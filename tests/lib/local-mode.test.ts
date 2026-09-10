import { afterEach, beforeEach, describe, expect, it } from 'vitest'

// In vitest `VITE_APP_MODE` is unset, so vite's `define` plugin compiles
// `__IS_TAURI__` to the literal `false` — i.e. the web build path. That's
// the path we actually need to lock in here: a web build must never treat
// "no bound device" as an authenticated local session. The Tauri branch
// is a one-liner (`__IS_TAURI__ && isLocalMode()`) and is exercised
// manually in the desktop app.
const loadLocalMode = async () => {
  const mod = await import('@/lib/device/local-mode')
  return mod
}

const loadClientId = async () => {
  const mod = await import('@/lib/device/client-id')
  return mod
}

describe('local-mode', () => {
  beforeEach(() => {
    sessionStorage.clear()
    localStorage.clear()
  })

  afterEach(() => {
    sessionStorage.clear()
    localStorage.clear()
  })

  it('isLocalModeAllowed() is false in web builds even when no device is bound', async () => {
    const lm = await loadLocalMode()
    const wc = await loadClientId()
    // Sanity: the underlying state really is "nothing bound" — and because
    // this compiles as a web build, isLocalMode() itself must report false.
    // The whole point of this test is that the helper refuses to act on the
    // "no bound device" state in a web build.
    expect(wc.getRemoteClientId()).toBe('')
    expect(wc.isLocalMode()).toBe(false)
    expect(lm.isLocalModeAllowed()).toBe(false)
  })

  it('isLocalModeAllowed() is false in web builds even after binding a device', async () => {
    const lm = await loadLocalMode()
    const wc = await loadClientId()
    wc.setRemoteClientId('device-xyz')
    expect(wc.isLocalMode()).toBe(false)
    expect(lm.isLocalModeAllowed()).toBe(false)
  })

  it('isLocalModeAllowed() and isLocalRouteGroup() are independent helpers', async () => {
    const lm = await loadLocalMode()
    expect(typeof lm.isLocalModeAllowed).toBe('function')
    expect(typeof lm.isLocalRouteGroup).toBe('function')
    expect(lm.isLocalRouteGroup('home')).toBe(true)
    expect(lm.isLocalRouteGroup('chat')).toBe(true)
    expect(lm.isLocalRouteGroup('files')).toBe(false)
  })
})
