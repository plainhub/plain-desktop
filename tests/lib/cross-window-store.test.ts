import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

/**
 * Simulates two Tauri webviews (or two browser tabs) sharing the same
 * BroadcastChannel by giving each its own sessionStorage values. We
 * `vi.resetModules()` between windows so each gets fresh module-level
 * caches, and pre-touch the cached getters so the identity is captured
 * before a later loadAsWindow() can stomp sessionStorage.
 *
 * This test only runs under the `cws` Vitest project (Node environment),
 * where `vi.resetModules()` actually invalidates the module cache. Under
 * the browser-mode `unit` project this approach cannot simulate two
 * independent windows — the page-side module cache is shared.
 */
const loadAsWindow = async (
  boundClientId: string | null,
  windowId: string,
  desktopClientId = 'desktop-1',
) => {
  sessionStorage.clear()
  if (boundClientId) sessionStorage.setItem('__bound_client_id__', boundClientId)
  sessionStorage.setItem('__window_id__', windowId)
  localStorage.setItem('client_id', desktopClientId)
  // `installed` (the WeakSet of stores that already had cross-window
  // sync attached) lives on globalThis so HMR / resetModules see the
  // same set. That means a previous test's store entries survive a
  // resetModules and silently short-circuit `installSync` on the next
  // window. Wipe it here so each simulated window starts from scratch.
  delete (globalThis as any).__plainWebInstalled
  vi.resetModules()
  const wc = await import('@/lib/device/client-id')
  wc.getWindowId()
  wc.getActiveClientId()
  const cws = await import('@/lib/cross-window-store')
  return { wc, cws }
}

describe('cross-window-store', () => {
  beforeEach(() => {
    sessionStorage.clear()
    localStorage.clear()
    // The cross-window-store module pins its `installed` WeakSet on
    // globalThis. Tests that run earlier may have already populated it
    // for a previously-defined store, and our loadAsWindow helper would
    // then attach a fresh store to a stale entry. Wipe it here so each
    // test starts with a clean install slate.
    delete (globalThis as any).__plainWebInstalled
    setActivePinia(createPinia())
  })

  it('publishes only the declared syncKeys', async () => {
    const { cws } = await loadAsWindow('device-1', 'win-A')
    const useStore = cws.defineCrossWindowStore<'kv-test', { a: number; b: number }>(
      'kv-test',
      { state: () => ({ a: 0, b: 0 }) },
      { syncKeys: ['a'] },
    )
    // Call useStore() inside win-A so installSync wires the publisher's
    // $subscribe with win-A's ownWindowId. If we defer this to after
    // loadAsWindow(win-B), installSync would attach to win-B's identity
    // and the self-echo filter would drop every broadcast.
    const pubStore = useStore()

    const received: Array<{ a: number; b: number }> = []
    const peer = await loadAsWindow('device-1', 'win-B')
    peer.cws.subscribeForTest('kv-test', (patch) => received.push(patch as any))

    pubStore.$patch({ a: 1, b: 99 })
    await new Promise((r) => setTimeout(r, 0))

    expect(received).toEqual([{ a: 1 }])
  })

  it('does not publish when only non-syncKeys mutate', async () => {
    // Regression: the publisher must watch only the declared syncKeys. A
    // whole-state deep watcher re-traverses the entire store on every
    // mutation tick — with tens of thousands of upload items in the temp
    // store that froze the UI solid during directory uploads.
    const { cws } = await loadAsWindow('device-1', 'win-A')
    const useStore = cws.defineCrossWindowStore<'kv-quiet', { a: number; b: number; items: number[] }>(
      'kv-quiet',
      { state: () => ({ a: 0, b: 0, items: [] as number[] }) },
      { syncKeys: ['a'] },
    )
    const pubStore = useStore()

    const received: Array<unknown> = []
    const peer = await loadAsWindow('device-1', 'win-B')
    peer.cws.subscribeForTest('kv-quiet', (patch) => received.push(patch))

    pubStore.$patch({ b: 42 })
    pubStore.items.push(1, 2, 3)
    await new Promise((r) => setTimeout(r, 0))

    expect(received).toEqual([])
  })

  it('filters out messages from a different clientId', async () => {
    const publisher = await loadAsWindow('device-1', 'win-A')
    const subscriber = await loadAsWindow('device-2', 'win-B')

    const received: Array<unknown> = []
    subscriber.cws.subscribeForTest('cid-test', (patch) => received.push(patch))

    publisher.cws.publishForTest('cid-test', { x: 1 }, 'device-1')
    await new Promise((r) => setTimeout(r, 0))
    expect(received).toEqual([])
  })

  it('filters out self-broadcasts (same windowId)', async () => {
    const w = await loadAsWindow('device-1', 'win-A')
    const received: Array<unknown> = []
    w.cws.subscribeForTest('self-test', (patch) => received.push(patch))

    w.cws.publishForTest('self-test', { y: 2 }, 'device-1')
    await new Promise((r) => setTimeout(r, 0))
    expect(received).toEqual([])
  })

  it('delivers messages to peers bound to the same device', async () => {
    const pub = await loadAsWindow('device-1', 'win-A')
    const sub = await loadAsWindow('device-1', 'win-B')

    const received: Array<unknown> = []
    sub.cws.subscribeForTest('same-test', (patch) => received.push(patch))

    pub.cws.publishForTest('same-test', { z: 3 }, 'device-1')
    await new Promise((r) => setTimeout(r, 0))
    expect(received).toEqual([{ z: 3 }])
  })

  it('delivers messages between local-mode windows sharing the desktop id', async () => {
    const pub = await loadAsWindow(null, 'win-A', 'desktop-1')
    const sub = await loadAsWindow(null, 'win-B', 'desktop-1')

    const received: Array<unknown> = []
    sub.cws.subscribeForTest('local-test', (patch) => received.push(patch))

    pub.cws.publishForTest('local-test', { local: true }, 'desktop-1')
    await new Promise((r) => setTimeout(r, 0))
    expect(received).toEqual([{ local: true }])
  })
})
