// HEVC (H.265) playback capability probing and fallback URL negotiation.
//
// Chromium browsers do not bundle an HEVC decoder: whether they can play one
// depends on OS-level codecs (Media Foundation + the HEVC extension on
// Windows, VideoToolbox on macOS, MediaCodec on Android); Safari ships its
// own. The only reliable signal is probing the running browser — never the
// User-Agent.

export const HEVC_HELP_URL = 'https://plainapp.app/docs/hevc'

function withQuery(url: string, query: string): string {
  return url + (url.includes('?') ? '&' : '?') + query
}

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

/** Ask the server (`probe=1`) which codec the video track uses. */
async function fetchVideoCodec(src: string): Promise<string> {
  const cached = codecCache.get(src)
  if (cached !== undefined) return cached
  let codec = ''
  try {
    const res = await fetch(withQuery(src, 'probe=1'))
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
 * transcoded (`tr=1`) H.264 stream; `transcoded` tells the UI to show the
 * "converting…" hint.
 */
export async function ensurePlayableVideoUrl(src: string): Promise<{ url: string; transcoded: boolean }> {
  if (browserSupportsHevc()) return { url: src, transcoded: false }
  const codec = await fetchVideoCodec(src)
  if (codec === 'hvc1' || codec === 'hev1') {
    return { url: withQuery(src, 'tr=1'), transcoded: true }
  }
  return { url: src, transcoded: false }
}

