/**
 * Cheap, synchronous platform detection from `navigator.userAgent`.
 *
 * These helpers are for UI affordances (e.g. "Cmd vs Ctrl" key labels)
 * that don't need the full `lib/agent/` AgentInfo parse. They run
 * synchronously so they can be used inside `computed()` initializers
 * without a flash of wrong content.
 */

const MAC_RE = /Mac|iPhone|iPad|iPod/
const WINDOWS_RE = /Windows NT/
const MOBILE_RE = /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini/i

export function isMacPlatform(): boolean {
  return typeof navigator !== 'undefined' && MAC_RE.test(navigator.userAgent)
}

export function isWindowsPlatform(): boolean {
  return typeof navigator !== 'undefined' && WINDOWS_RE.test(navigator.userAgent)
}

export function isMobilePlatform(): boolean {
  return typeof navigator !== 'undefined' && MOBILE_RE.test(navigator.userAgent)
}
