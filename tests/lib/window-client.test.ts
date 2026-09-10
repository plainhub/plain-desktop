import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// Each test resets the module cache so cached singletons in client-id.ts
// (cachedRemote / cachedWindowId) start fresh and pick up the new
// sessionStorage from the test setup's reset.
const loadClientId = async () => {
  vi.resetModules()
  const mod = await import('@/lib/device/client-id')
  return mod
}

describe('client-id', () => {
  beforeEach(() => {
    sessionStorage.clear()
    localStorage.clear()
  })

  afterEach(() => {
    sessionStorage.clear()
    localStorage.clear()
  })

  it('getRemoteClientId returns "" when no device is bound', async () => {
    const wc = await loadClientId()
    expect(wc.getRemoteClientId()).toBe('')
  })

  it('getActiveClientId falls back to the desktop clientId when nothing is bound', async () => {
    localStorage.setItem('client_id', 'desktop-abc')
    const wc = await loadClientId()
    expect(wc.getActiveClientId()).toBe('desktop-abc')
    // Web build: local mode is desktop-app-only (__IS_TAURI__ is false here).
    expect(wc.getRemoteClientId()).toBe('')
    expect(wc.isLocalMode()).toBe(false)
  })

  it('getActiveClientId returns the bound device id when one is set', async () => {
    localStorage.setItem('client_id', 'desktop-abc')
    const wc = await loadClientId()
    wc.setRemoteClientId('device-xyz')
    expect(wc.getRemoteClientId()).toBe('device-xyz')
    expect(wc.getActiveClientId()).toBe('device-xyz')
    expect(wc.isLocalMode()).toBe(false)
  })

  it('clearRemoteClientId drops the window back to the desktop clientId', async () => {
    localStorage.setItem('client_id', 'desktop-abc')
    const wc = await loadClientId()
    wc.setRemoteClientId('device-xyz')
    wc.clearRemoteClientId()
    expect(wc.getRemoteClientId()).toBe('')
    expect(wc.getActiveClientId()).toBe('desktop-abc')
    // Web build: local mode is desktop-app-only (__IS_TAURI__ is false here).
    expect(wc.isLocalMode()).toBe(false)
  })

  it('setRemoteClientId("") is equivalent to clear', async () => {
    localStorage.setItem('client_id', 'desktop-abc')
    const wc = await loadClientId()
    wc.setRemoteClientId('device-xyz')
    wc.setRemoteClientId('')
    expect(wc.getRemoteClientId()).toBe('')
  })

  it('getWindowId is stable across module reloads and unique per window', async () => {
    const a = (await loadClientId()).getWindowId()
    const b = (await loadClientId()).getWindowId()
    expect(a).toBe(b)
    expect(sessionStorage.getItem('__window_id__')).toBe(a)
  })

  it('isLocalClientId compares against the desktop clientId', async () => {
    localStorage.setItem('client_id', 'desktop-abc')
    const wc = await loadClientId()
    expect(wc.isLocalClientId('')).toBe(true)
    expect(wc.isLocalClientId('desktop-abc')).toBe(true)
    expect(wc.isLocalClientId('some-device')).toBe(false)
  })

  it('applyUrlClientId writes __cid into the bound sessionStorage and strips it from URL', async () => {
    localStorage.setItem('client_id', 'desktop-abc')
    window.history.replaceState({}, '', '/?__cid=device-42&other=keep')
    const wc = await loadClientId()
    wc.applyUrlClientId()
    expect(wc.getRemoteClientId()).toBe('device-42')
    expect(wc.getActiveClientId()).toBe('device-42')
    expect(wc.isLocalMode()).toBe(false)
    const after = window.location.search
    expect(after).not.toContain('__cid')
    expect(after).toContain('other=keep')
  })

  it('applyUrlClientId is a no-op when there is no __cid', async () => {
    localStorage.setItem('client_id', 'desktop-abc')
    window.history.replaceState({}, '', '/?x=1')
    const wc = await loadClientId()
    const before = wc.getActiveClientId()
    wc.applyUrlClientId()
    expect(wc.getActiveClientId()).toBe(before)
  })

  it('applyUrlClientId drops a device __cid when the desktop client_id is not loaded yet', async () => {
    // Contract behind main.ts bootstrap ordering: before prefs are preloaded
    // (Tauri plugin-store), getDesktopClientId() reads as '' and any __cid is
    // treated as local and stripped. That is why applyUrlClientId() must run
    // AFTER preloadPrefs() — otherwise child windows lose their device
    // binding and fall back to the local server (wrong /fs host → 401).
    window.history.replaceState({}, '', '/?__cid=device-42')
    const wc = await loadClientId()
    wc.applyUrlClientId()
    // Assert on sessionStorage, not getRemoteClientId(): the module-level
    // cachedRemote can leak across tests in browser mode (vi.resetModules
    // doesn't always re-evaluate the module), so the getter is unreliable —
    // but nothing being written to sessionStorage is the real contract.
    expect(sessionStorage.getItem('__bound_client_id__')).toBe(null)
    expect(window.location.search).not.toContain('__cid')
  })
})