import { FileAction, withFileAction } from './api/file-action'
import { isNativeResourceUrl } from '@/hooks/native-resource-url'

// HEVC (H.265) playback capability probing and fallback URL negotiation.
//
// Chromium browsers do not bundle an HEVC decoder: whether they can play one
// depends on OS-level codecs (Media Foundation + the HEVC extension on
// Windows, VideoToolbox on macOS, MediaCodec on Android); Safari ships its
// own. The only reliable signal is probing the running browser — never the
// User-Agent.

export const HEVC_HELP_URL = 'https://plainapp.app/docs/hevc'

let hevcSupported: boolean | null = null

export function browserSupportsHevc(): boolean {
  if (hevcSupported === null) {
    const probe = document.createElement('video')
    // hvc1 Main-profile: the canonical probe string. An empty string means
    // definitively unsupported; 'maybe'/'probably' both mean decodable.
    hevcSupported = probe.canPlayType('video/mp4; codecs="hvc1.1.6.L93.B0"') !== ''
  }
  return hevcSupported
}

const codecCache = new Map<string, string>()

/** Ask the server (`action=probe`) which codec the video track uses. */
async function fetchVideoCodec(src: string, chat: boolean): Promise<string> {
  const cached = codecCache.get(src)
  if (cached !== undefined) return cached
  let codec = ''
  try {
    const res = await fetch(withFileAction(src, FileAction.Probe, chat))
    if (res.ok) codec = ((await res.json()) as { codec?: string }).codec ?? ''
  } catch {
    // A failed probe falls back to the original URL; the <video> error path
    // still catches hard failures.
  }
  codecCache.set(src, codec)
  return codec
}

/**
 * Resolve the URL a video should actually load. On browsers without HEVC
 * support, HEVC sources are transparently switched to the server's
 * transcoded (`action=transcode`) H.264 stream; `transcoded` tells the UI to show the
 * "converting…" hint.
 */
export async function ensurePlayableVideoUrl(src: string, chat = false): Promise<{ url: string; transcoded: boolean }> {
  // Native handles expose bytes, not the HTTP codec probe or transcode actions.
  if (isNativeResourceUrl(src) || browserSupportsHevc()) return { url: src, transcoded: false }
  const codec = await fetchVideoCodec(src, chat)
  if (codec === 'hvc1' || codec === 'hev1') {
    return { url: withFileAction(src, FileAction.Transcode, chat), transcoded: true }
  }
  return { url: src, transcoded: false }
}
