// WebCodecs requires cross-origin isolation for the hardware-accelerated
// VideoDecoder/AudioDecoder paths — without these headers `crossOriginIsolated`
// is false, `decode()` throws `Decoder error` even though `isConfigSupported`
// reports supported. `credentialless` is preferred over `require-corp` because
// it does not block cross-origin subresources (avatars, fonts, etc.).
//
// Browsers discard COOP on non-trustworthy origins (plain HTTP over a LAN IP)
// and log a console warning, so the dev server must only emit them for hosts
// whose origin is potentially trustworthy (localhost / loopback) —
// https://www.w3.org/TR/powerful-features/#potentially-trustworthy-origin.
export const crossOriginIsolationHeaders = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'credentialless',
} as const

export function isPotentiallyTrustworthyHost(forwardedHost: string | undefined): boolean {
  if (!forwardedHost) return false
  let hostname = forwardedHost.trim().toLowerCase()
  if (hostname.startsWith('[')) {
    const end = hostname.indexOf(']')
    if (end === -1) return false
    hostname = hostname.slice(1, end)
  } else if (hostname.split(':').length === 2) {
    hostname = hostname.slice(0, hostname.indexOf(':'))
  }
  return (
    hostname === 'localhost' ||
    hostname.endsWith('.localhost') ||
    hostname === '::1' ||
    hostname.startsWith('127.')
  )
}
