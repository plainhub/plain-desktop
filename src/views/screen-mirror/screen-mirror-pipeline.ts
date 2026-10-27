import { ref, type Ref, onScopeDispose } from 'vue'
import { ScreenMirrorVideoPipeline } from '@/lib/mirror-codec-video'
import { ScreenMirrorAudioPipeline } from '@/lib/mirror-codec-audio'
import { isVideoDecodeSupported, isAudioDecodeSupported, webCodecsMissing } from '@/lib/mirror-codec-support'
import { gqlFetch } from '@/lib/api/gql-client'
import { screenMirrorVideoCodecGQL, requestScreenMirrorKeyFrameGQL } from '@/lib/api/query'
import { base64ToArrayBuffer } from '@/lib/strutil'
import { parseVideoPacket, type VideoPacket } from '@/lib/video-packet'

export interface ScreenMirrorVideoCodec {
  annexB: string
  keyFrame?: string
}

/**
 * Binary mirror pipeline: CONFIG (Annex-B SPS+PPS) pulled via GraphQL
 * `screenMirrorVideoCodec`; video NAL units decoded by WebCodecs VideoDecoder
 * and drawn to canvas; audio Opus packets decoded into <audio>.
 *
 * All video/audio frames arrive as VideoPacket-wrapped binary WebSocket events
 * (see lib/video-packet.ts). The packet carries frameId (loss detection),
 * timestamp (A/V sync), and flags (keyframe/audio) — the web side no longer
 * generates its own PTS clock.
 */
export function useScreenMirrorPipeline(
  canvasRef: Ref<HTMLCanvasElement | undefined>,
  audioRef: Ref<HTMLAudioElement | undefined>,
  onFirstFrame: () => void,
  onDisconnected: () => void,
  onScreenMirrorOff: () => void
) {
  const video = new ScreenMirrorVideoPipeline()
  const audio = new ScreenMirrorAudioPipeline()
  const supported = ref(isVideoDecodeSupported())
  const audioSupported = ref(isAudioDecodeSupported())
  const paused = ref(false)
  let connected = false
  let generation = 0
  // Cached codec config so the video decoder can be re-created on error
  // without a round-trip back to GraphQL. The encoder emits an IDR on demand
  // (via requestScreenMirrorKeyFrame mutation), so once we mark the decoder
  // broken we drop P-frames until the next IDR arrives and re-configure with
  // the same SPS/PPS.
  let cachedConfig: Uint8Array | null = null
  let decoderNeedsReset = false

  onScopeDispose(() => {
    video.close()
    audio.close()
  })

  function b64ToBytes(b64: string): Uint8Array {
    return new Uint8Array(base64ToArrayBuffer(b64))
  }

  async function connect() {
    cleanup()
    const current = generation
    // Video-only gate: on WebKit < 26 (e.g. macOS 15 WKWebView) AudioDecoder
    // doesn't exist, but VideoDecoder does — audio absence must not block video.
    if (!isVideoDecodeSupported()) {
      console.error(`[MirrorPipeline] WebCodecs video not supported, missing: ${webCodecsMissing().join(', ')}`)
      onDisconnected()
      return
    }
    const canvas = canvasRef.value
    const audioEl = audioRef.value
    if (!canvas) {
      console.error('[MirrorPipeline] canvas not ready')
      onDisconnected()
      return
    }
    video.attach(canvas)
    if (audioEl) audio.attach(audioEl)
    // Pull the codec config proactively; if the phone isn't mirroring, the
    // GraphQL field returns null and we abort.
    try {
      const r: any = await gqlFetch(screenMirrorVideoCodecGQL, {})
      if (current !== generation) return
      const annexB = r?.data?.screenMirrorVideoCodec?.annexB ?? null
      if (!annexB) {
        console.warn('[MirrorPipeline] no screenMirrorVideoCodec from server')
        onScreenMirrorOff()
        return
      }
      const config = b64ToBytes(annexB)
      console.log(`[MirrorPipeline] pulled config ${config.byteLength}B, requesting fresh IDR`)
      cachedConfig = config
      decoderNeedsReset = false
      video.configure(config)
      video.setOnError(() => {
        decoderNeedsReset = true
      })
      video.setOnRequestKeyFrame(requestKeyFrame)
      video.setOnFirstFrameRendered(onFirstFrame)
      // Don't decode the stale GraphQL keyframe. The encoder's first IDR is
      // produced before VirtualDisplay renders real screen content, so it's
      // a blank (green) frame. Instead, set waitingForIdr so P-frames are
      // dropped, then request a fresh IDR — by the time it arrives the
      // VirtualDisplay has real content.
      video.requestIdr()
      // Audio must never block video: a decoder recreate loop or an engine
      // without Opus AudioDecoder support would otherwise leave `connected`
      // false and silently drop every video packet.
      try {
        audio.prepare()
      } catch (e) {
        console.error('[MirrorPipeline] audio prepare failed, continuing video-only', e)
      }
      connected = true
      await requestKeyFrame()
    } catch (e) {
      if (current !== generation) return
      console.error('[MirrorPipeline] failed to fetch screenMirrorVideoCodec', e)
      onDisconnected()
    }
  }

  function handleVideo(rawData: Uint8Array) {
    if (!connected || paused.value) return
    const packet = parseVideoPacket(rawData)
    if (!packet || packet.isAudio) return
    // If the decoder errored, drop everything until the next IDR, then
    // re-configure with the cached config (creates a fresh VideoDecoder in
    // 'configured' state — see mirror-codec.configure) before decoding the IDR.
    if (decoderNeedsReset) {
      if (!packet.isKeyFrame || !cachedConfig) return
      decoderNeedsReset = false
      video.configure(cachedConfig)
      console.log('[MirrorPipeline] decoder reset on IDR')
    }
    video.decode(packet)
  }

  function handleAudio(rawData: Uint8Array) {
    if (!connected || paused.value) return
    const packet = parseVideoPacket(rawData)
    if (!packet || !packet.isAudio) return
    audio.decode(packet.data, packet.timestamp)
  }

  const togglePlay = () => {
    paused.value = !paused.value
  }

  const enableAudio = () => audio.enable()
  const isAudioEnabled = () => audio.isEnabled()
  const setAudioMuted = (m: boolean) => audio.setMuted(m)

  function handleConfig(codec: ScreenMirrorVideoCodec | null) {
    // Android embeds the new Annex-B SPS+PPS in the event payload, so we
    // can reconfigure the decoder directly without a GraphQL round-trip.
    // Canvas sizing is implicit in renderFrame (frame.displayWidth/Height),
    // so the web side doesn't need to know width/height/rotation.
    if (!supported.value || !connected) return
    if (!codec?.annexB) return
    const newConfig = b64ToBytes(codec.annexB)
    // Skip if the config is byte-identical to what the decoder already has —
    // configure() with the same bytes is a no-op but still resets the
    // decoder and costs an IDR to recover.
    const sameAsCached = cachedConfig && cachedConfig.byteLength === newConfig.byteLength && cachedConfig.every((b, i) => b === newConfig[i])
    if (sameAsCached) return
    cachedConfig = newConfig
    decoderNeedsReset = false
    video.configure(newConfig)
    if (codec.keyFrame) {
      video.decode(makeSyntheticKeyFrame(b64ToBytes(codec.keyFrame)))
    }
    // Drop any residual P-frames from the old encoder (still in the WebSocket
    // pipeline) until a fresh IDR arrives — they reference the old SPS/PPS and
    // will produce artifacts. Then request a clean IDR that is guaranteed to
    // have the post-resize dimensions.
    video.requestIdr()
    requestKeyFrame()
    console.log(`[MirrorPipeline] reconfigured decoder on config event ${newConfig.byteLength}B${codec.keyFrame ? ` + keyFrame ${codec.keyFrame.length}B` : ''}`)
  }

  async function requestKeyFrame() {
    try {
      await gqlFetch(requestScreenMirrorKeyFrameGQL, {})
    } catch (e) {
      console.error('[MirrorPipeline] requestKeyFrame failed', e)
    }
  }

  function cleanup() {
    generation++
    video.close()
    audio.close()
    connected = false
  }

  return {
    supported,
    audioSupported,
    paused,
    connect,
    handleVideo,
    handleAudio,
    handleConfig,
    cleanup,
    togglePlay,
    enableAudio,
    isAudioEnabled,
    setAudioMuted,
  }
}

/**
 * Build a synthetic VideoPacket for a keyframe pulled via GraphQL (not wrapped
 * in the VideoPacket protocol since it travels over HTTP, not WebSocket).
 * frameId=0 and timestamp=0 are safe because the video pipeline's loss
 * detection only fires when lastFrameId > 0, and keyframes bypass the
 * timestamp scheduler.
 */
function makeSyntheticKeyFrame(data: Uint8Array): VideoPacket {
  return {
    frameId: 0,
    timestamp: 0,
    isKeyFrame: true,
    isConfig: false,
    isAudio: false,
    data,
  }
}

if (import.meta.hot) {
  import.meta.hot.accept(() => {
    window.location.reload()
  })
}
