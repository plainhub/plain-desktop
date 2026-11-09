import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createRenderer, nextTick, ref, ssrContextKey, type Ref } from 'vue'
import type { IFeedEntry } from '@/lib/interfaces'

const fixture = vi.hoisted(() => ({ items: undefined as unknown as Ref<IFeedEntry[]>, invoke: vi.fn(), local: true, peer: 'peer' }))
vi.mock('@/lib/format', () => ({ formatTimeAgo: vi.fn(), formatDateTime: vi.fn() }))
vi.mock('@/lib/api/file', () => ({ resolveFileUrlByPath: (owner: any, _key: unknown, path: string, size: number, query: string) => owner.url(path, size, query) }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: fixture.invoke }))
vi.mock('@/lib/api/native-client', () => ({ nativeViewScope: async () => 'document' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => fixture.local }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => fixture.peer }))
vi.mock('@/lib/device/login-peers', () => ({ getLoginPeerRevision: () => 0 }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ urlTokenKey: null }) }))
vi.mock('@/views/feeds/hooks/useFeedEntriesData', () => ({ useFeedEntriesData: () => ({ items: fixture.items }) }))
vi.mock('@/views/feeds/hooks/useFeedEntriesActions', () => ({ useFeedEntriesActions: () => ({}) }))
vi.mock('@/components/virtualscroll', () => ({ default: {} }))
import { createNativeResourceOwner } from '@/hooks/native-resource-url'
import FeedsSidebar2 from '@/views/feeds/FeedsSidebar2.vue'
FeedsSidebar2.render = () => null

const renderer = createRenderer<object, object>({
  insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
})
let unmount: (() => void) | undefined
const live = new Set<string>()
let nextId = 0
function opened(path: string) {
  const id = String(++nextId); live.add(id)
  return { id, url: `plain-resource://file/${id}`, name: path, size: 4, mime: 'image/png', inlineAllowed: true }
}
function mount() {
  const app = renderer.createApp(FeedsSidebar2)
  app.provide(ssrContextKey, { modules: new Set() })
  app.provide('isPhone', false)
  app.mount({}); unmount = () => app.unmount()
}
async function flush() { for (let i = 0; i < 16; i++) await nextTick() }
function setImages(...paths: string[]) { fixture.items.value = paths.map((image, index) => ({ id: String(index), image }) as IFeedEntry) }
function delayedImages() {
  const pending = new Map<string, () => void>()
  fixture.invoke.mockImplementation((command: string, args: any) => {
    if (command === 'native_resource_open') return new Promise(resolve => pending.set(args.path, () => resolve(opened(args.path))))
    live.delete(args.id); return Promise.resolve(true)
  })
  return (path: string) => pending.get(path)!()
}

beforeEach(() => {
  vi.clearAllMocks(); live.clear(); fixture.items = ref([]); fixture.local = true
  vi.stubGlobal('__IS_TAURI__', true)
  fixture.invoke.mockImplementation(async (command: string, args: any) => {
    if (command === 'native_resource_open') return opened(args.path)
    live.delete(args.id); return true
  })
  vi.spyOn(console, 'warn').mockImplementation(() => {})
})
afterEach(async () => {
  unmount?.(); unmount = undefined; await flush()
  vi.restoreAllMocks(); vi.unstubAllGlobals()
})

it('closes pending thumbnail results after unmount without publishing stale images', async () => {
  const finish = delayedImages()
  setImages('first', 'second'); mount(); await flush()
  unmount?.(); unmount = undefined
  finish('first'); finish('second'); await flush()
  expect(live.size).toBe(0)
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_close')).toHaveLength(2)
  expect(console.warn).not.toHaveBeenCalled()
})

it('releases replaced pending images while sharing the latest list reference', async () => {
  const finish = delayedImages()
  setImages('old', 'shared'); mount(); await flush()
  setImages('new', 'shared'); await flush()
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_open')).toHaveLength(3)
  finish('new'); finish('shared'); await flush()
  expect(live.size).toBe(2)
  finish('old'); await flush()
  expect(live.size).toBe(2)
  unmount?.(); unmount = undefined; await flush()
  expect(live.size).toBe(0)
})

it('preserves external URLs and cleans successful opens when a sibling fails', async () => {
  fixture.invoke.mockImplementation(async (command: string, args: any) => {
    if (command === 'native_resource_open') {
      if (args.path === 'broken') throw new Error('unavailable')
      return opened(args.path)
    }
    live.delete(args.id); return true
  })
  setImages('https://feed.invalid/image.png', 'image', 'broken'); mount(); await flush()
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_open')).toHaveLength(2)
  expect(console.warn).toHaveBeenCalledWith('[FeedsSidebar2] Failed to load image', expect.any(Error))
  unmount?.(); unmount = undefined; await flush()
  expect(live.size).toBe(0)
})

it('does not close another consumer of the same thumbnail when feeds unmounts', async () => {
  const other = createNativeResourceOwner()
  await other.url('shared', undefined, '&size=sm')
  setImages('shared'); mount(); await flush()
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_open')).toHaveLength(1)
  unmount?.(); unmount = undefined; await flush()
  expect(live.size).toBe(1)
  await other.dispose()
  expect(live.size).toBe(0)
})


it('reacquires the current target when a refreshed list reuses the same reference', async () => {
  setImages('same'); mount(); await flush()
  fixture.local = false
  setImages('same'); await flush()
  const opens = fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_open')
  expect(opens.map(([, args]) => args.target)).toEqual([{ kind: 'local' }, { kind: 'peer', peerId: 'peer' }])
  expect(live.size).toBe(1)
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_close')).toHaveLength(1)
})
