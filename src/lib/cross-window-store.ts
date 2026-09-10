/**
 * Pinia store factory that automatically mirrors a declared subset of state
 * across all windows/tabs of the same app via BroadcastChannel.
 *
 * Usage:
 *   export const useMyStore = defineCrossWindowStore<MyState>('my', {
 *     state: () => ({ ... } as MyState),
 *   }, {
 *     syncKeys: ['counter', 'audioPlaying'],
 *   })
 *
 * Semantics:
 *   - Only the keys listed in `syncKeys` are broadcast. Everything else stays
 *     per-window (uploads, lightbox, app config, ...).
 *   - Each BroadcastChannel message carries `{ windowId, clientId, patch }`.
 *     The receiver drops the message when:
 *       - `windowId === getWindowId()` (self-broadcast echo), OR
 *       - `clientId !== getActiveClientId()` (different window identity).
 *     Same-clientId-only filtering means a window bound to device A will not
 *     pick up state from a window bound to device B.
 *   - Remote applications go through `$patch` with a replay guard
 *     (`__cw_replaying`) so the receiver's own `$subscribe` does not echo
 *     the patch back out.
 *   - On the publisher side, only mutations are observed (not the deep watch)
 *     so unrelated state changes don't spam the channel.
 *
 * Works in both web tabs (same origin BroadcastChannel) and Tauri webviews
 * (same origin within the app process). No platform branching in this file.
 */
import { defineStore, type Store, type _GettersTree, type _ActionsTree } from 'pinia'
import { toRaw, watch } from 'vue'
import { getActiveClientId, getWindowId } from '@/lib/device/client-id'

const CHANNEL_PREFIX = 'plain-web:store:'

interface SyncMessage {
  windowId: string
  clientId: string
  patch: Record<string, unknown>
}

interface ChannelEntry {
  bc: BroadcastChannel
  /** Subscribers receive the raw patch (already filtered for windowId + clientId). */
  subscribers: Set<(patch: Record<string, unknown>) => void>
}

// Each window owns its own channels Map (module-level). BroadcastChannel
// instances created in different windows still talk to each other because
// the runtime keys fan-out by channel name (per the BroadcastChannel spec).
const channels = new Map<string, ChannelEntry>()

function getOrCreateChannel(name: string): ChannelEntry {
  let entry = channels.get(name)
  if (entry) return entry
  const bc = new BroadcastChannel(CHANNEL_PREFIX + name)
  const subscribers = new Set<(p: Record<string, unknown>) => void>()
  bc.onmessage = (ev: MessageEvent<SyncMessage>) => {
    const m = ev.data
    if (!m || m.windowId === getWindowId()) return
    if (m.clientId !== getActiveClientId()) return
    for (const fn of subscribers) fn(m.patch)
  }
  entry = { bc, subscribers }
  channels.set(name, entry)
  return entry
}

/**
 * Test-only helpers. They expose the internal channel registry so unit tests
 * can simulate peer windows without spawning real BroadcastChannel peers.
 * They use the same windowId / clientId filter as the live message handler.
 */
export const publishForTest = (
  name: string,
  patch: Record<string, unknown>,
  clientId?: string,
  windowId?: string,
) => {
  const entry = getOrCreateChannel(name)
  const msg: SyncMessage = {
    windowId: windowId ?? getWindowId(),
    clientId: clientId ?? getActiveClientId(),
    patch,
  }
  entry.bc.postMessage(msg)
}

export const subscribeForTest = (
  name: string,
  fn: (patch: Record<string, unknown>) => void,
): (() => void) => {
  const entry = getOrCreateChannel(name)
  entry.subscribers.add(fn)
  return () => entry.subscribers.delete(fn)
}

export interface CrossWindowOptions<S> {
  /**
   * State keys whose value is mirrored across windows/tabs.
   * Unlisted keys remain strictly per-window (uploads, lightbox, etc.).
   */
  syncKeys: (keyof S)[]
}

// Tracks installed stores so we don't double-subscribe. Pinned on globalThis
// so module reloads (HMR / `vi.resetModules()`) see the same set.
const installed: WeakSet<object> =
  (globalThis as any).__plainWebInstalled ?? new WeakSet<object>()
;(globalThis as any).__plainWebInstalled = installed

function installSync<S extends object>(
  store: Store<string, S, _GettersTree<S>, _ActionsTree>,
  channelName: string,
  syncKeys: (keyof S)[],
): void {
  if (installed.has(store)) return
  installed.add(store)

  const channel = getOrCreateChannel(channelName)

  channel.subscribers.add((patch) => {
    ;(store as any).__cw_replaying = true
    try {
      // pinia's $patch expects _DeepPartial<UnwrapRef<S>>; our patch comes
      // off BroadcastChannel as a plain object, so cast through unknown.
      ;(store as any).$patch(patch)
    } finally {
      // Microtask defer so any watcher triggered by $patch sees the flag
      // synchronously, then we clear it before the next event-loop turn.
      queueMicrotask(() => {
        ;(store as any).__cw_replaying = false
      })
    }
  })

  const publish = () => {
    if ((store as any).__cw_replaying) return
    const raw: Record<string, unknown> = {}
    for (const k of syncKeys) raw[k as string] = toRaw((store as any).$state[k])
    // BroadcastChannel.postMessage runs structured-clone, which rejects
    // Vue reactive proxies. `syncKeys` is documented to be plain JSON
    // data, so a JSON round-trip is the safe way to detach the patch
    // from the reactive graph. (toRaw on its own isn't enough for
    // nested objects inside `counter`.)
    const patch = JSON.parse(JSON.stringify(raw))
    const msg: SyncMessage = {
      windowId: getWindowId(),
      clientId: getActiveClientId(),
      patch,
    }
    channel.bc.postMessage(msg)
  }

  // Watch only the declared sync keys. Pinia's $subscribe is a deep watcher
  // over the ENTIRE state tree — with tens of thousands of upload items in
  // the temp store, every mutation tick re-traversed the whole graph (~100ms
  // per tick at 17k items) and froze the UI solid during directory uploads.
  // Object keys get a deep watch (they are small: counter, etc.); primitive
  // keys only need the default shallow observation.
  for (const k of syncKeys) {
    const value = (store as any).$state[k]
    const deep = typeof value === 'object' && value !== null
    watch(() => (store as any).$state[k], publish, { deep })
  }
}

/**
 * Wraps a Pinia store definition with automatic cross-window sync for the
 * declared keys. Call sites use `useStore()` exactly like a normal Pinia
 * store — the cross-window wiring is set up the first time `useStore()`
 * is called inside an active pinia instance.
 *
 * The return type is `typeof defineStore(...)` so call sites stay generic.
 */
export function defineCrossWindowStore<
  Id extends string,
  S extends object,
  G extends _GettersTree<S> = {},
  A extends _ActionsTree = {},
>(
  id: Id,
  options: { state: () => S; getters?: G; actions?: A },
  crossWindow: CrossWindowOptions<S>,
): () => Store<Id, S, G, A> {
  const useStoreImpl = defineStore(id, options as any)
  const { syncKeys } = crossWindow

  return (() => {
    const store = useStoreImpl() as unknown as Store<string, S, _GettersTree<S>, _ActionsTree>
    installSync(store, id, syncKeys)
    return store as unknown as Store<Id, S, G, A>
  }) as () => Store<Id, S, G, A>
}
