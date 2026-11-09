import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createRenderer, defineComponent, nextTick, reactive, ref } from 'vue'
import type { ISource } from '@/components/lightbox/types'

const fixture = vi.hoisted(() => ({ invoke: vi.fn(), store: undefined as any }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: fixture.invoke }))
vi.mock('@/lib/api/native-client', () => ({ nativeViewScope: async () => 'document' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => true }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => '' }))
vi.mock('@/lib/device/login-peers', () => ({ getLoginPeerRevision: () => 0 }))
vi.mock('@/lib/api/file', async importOriginal => ({
  ...await importOriginal<typeof import('@/lib/api/file')>(), download: vi.fn(),
}))
vi.mock('@/lib/api/api', () => ({
  getProxyUrl: (path: string) => `http://127.0.0.1:8910${path}`,
  getApiHeaders: () => ({ 'c-id': 'fixture' }), getUrlToken: vi.fn(),
}))
vi.mock('@/components/toaster', () => ({ default: vi.fn(), toastWithAction: vi.fn() }))
vi.mock('@/lib/download-dir', () => ({ downloadToDir: vi.fn(), downloadAs: vi.fn(), revealInFolder: vi.fn() }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => fixture.store }))
vi.mock('@/stores/main', () => ({ useMainStore: vi.fn() }))
vi.mock('@/lib/api/query', () => ({ initLazyQuery: vi.fn(), fileInfoGQL: '', tagRelationsGQL: '', tagsGQL: '' }))
vi.mock('@/hooks/files', () => ({ useRename: vi.fn() }))
vi.mock('@/hooks/media', () => ({ useDeleteItems: vi.fn() }))
vi.mock('@/hooks/media-play-history', () => ({ useMediaPlayHistory: vi.fn() }))
vi.mock('@/components/modal', () => ({ openModal: vi.fn() }))
vi.mock('@/components/DeleteFileConfirm.vue', () => ({ default: {} }))
vi.mock('@/components/EditValueModal.vue', () => ({ default: {} }))
vi.mock('@/lib/video-codec', () => ({ browserSupportsHevc: () => true, ensurePlayableVideoUrl: async (url: string) => ({ url, transcoded: false }) }))
import { createNativeResourceOwner } from '@/hooks/native-resource-url'
import { useLightboxNavigation } from '@/hooks/lightbox'

const renderer = createRenderer<object, object>({
  insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
})
let unmount: (() => void) | undefined
let nav: ReturnType<typeof useLightboxNavigation>
let caseId = 0
let id = 0
let status: { loading: boolean; loadError: boolean; dragging: boolean; gesturing: boolean }
let loop = ref(false)
const images: { src: string; onload: (() => void) | null; onerror: (() => void) | null; removeAttribute: () => void }[] = []
const videos: any[] = []
const live = new Set<string>()
function source(index: number): ISource { return { path: `/images/${caseId}/${index}.png`, src: '', name: `${index}.png`, size: 4, durationMs: 0, updatedAt: 'version' } }
function mount(count = 100) {
  fixture.store = reactive({ urlTokenKey: new Uint8Array(32).fill(7), lightbox: { visible: true, index: -1, sources: Array.from({ length: count }, (_, i) => source(i)) } })
  const app = renderer.createApp(defineComponent({
    setup() {
      status = reactive({ loading: false, loadError: false, dragging: false, gesturing: false })
      loop = ref(false)
      nav = useLightboxNavigation(fixture.store, ref(0), ref(), reactive({}) as any,
        status, new Map(), vi.fn(), vi.fn(), vi.fn(), loop, vi.fn(), ref('original'))
      return () => null
    },
  }))
  app.mount({}); unmount = () => app.unmount()
}
async function flush() { for (let i = 0; i < 16; i++) await nextTick() }

beforeEach(() => {
  caseId++; id = 0; live.clear(); vi.clearAllMocks()
  vi.stubGlobal('__IS_TAURI__', true)
  vi.stubGlobal('window', { fileIdMap: new Map() })
  images.length = 0; videos.length = 0
  vi.stubGlobal('Image', class { src = ''; onload = null; onerror = null; constructor() { images.push(this) }; removeAttribute() { this.src = '' } })
  vi.stubGlobal('document', { createElement: () => {
    const listeners = new Map<string, () => void>()
    const video = { src: '', load: vi.fn(), listeners,
      removeAttribute: vi.fn(() => { video.src = '' }),
      addEventListener: vi.fn((event: string, listener: () => void) => listeners.set(event, listener)),
      removeEventListener: vi.fn((event: string) => listeners.delete(event)),
    }
    videos.push(video); return video
  } })
  fixture.invoke.mockImplementation(async (command: string, args: any) => {
    if (command === 'native_resource_open') {
      const resourceId = String(++id); live.add(resourceId)
      return { id: resourceId, url: `plain-resource://file/${resourceId}`, name: args.path, mime: args.path.endsWith('.mp4') ? 'video/mp4' : args.path.endsWith('.mp3') ? 'audio/mpeg' : 'image/png', size: 4, inlineAllowed: true }
    }
    if (command === 'native_resource_close') live.delete(args.id)
    return true
  })
})
afterEach(() => { unmount?.(); unmount = undefined; vi.unstubAllGlobals(); vi.restoreAllMocks() })

it('keeps only current and actual neighbour handles while browsing, then releases them on close', async () => {
  mount()
  for (let index = 0; index < 100; index++) {
    await nav.changeIndex(index); await flush()
    expect(live.size).toBeLessThanOrEqual(4)
  }
  nav.closeDialog(); await flush()
  expect(live.size).toBe(0)
})

it('releases handles when the component unmounts without closing the dialog first', async () => {
  mount(4); await nav.changeIndex(0); await flush()
  expect(live.size).toBeGreaterThan(0)
  unmount?.(); unmount = undefined; await flush()
  expect(live.size).toBe(0)
})

it('reopens evicted sources instead of using retired source and playback URLs', async () => {
  mount(20); await nav.changeIndex(0); await flush()
  const first = fixture.store.lightbox.sources[0]
  const originalUrl = first.src
  await nav.changeIndex(10); await flush()
  expect(first.src).toBe('')
  await nav.changeIndex(0); await flush()
  expect(first.src).not.toBe(originalUrl)
  expect(live.size).toBeLessThanOrEqual(4)
  expect(images.filter(image => image.src === '').every(image => image.onload === null)).toBe(true)
})

it('retires the old source list even when the replacement has the same index', async () => {
  mount(4)
  fixture.store.lightbox.index = 0; await flush()
  const old = fixture.store.lightbox.sources
  expect(live.size).toBe(3)
  fixture.store.lightbox = { visible: true, index: 0, sources: [source(100)] }
  await flush()
  expect(old.every((item: ISource) => !item.src)).toBe(true)
  expect(live.size).toBe(1)
})

it('closes a late native open after the lightbox is dismissed without assigning its URL', async () => {
  mount(1)
  const invoke = fixture.invoke.getMockImplementation()!
  let finish!: () => void
  fixture.invoke.mockImplementation((command: string, args: any) => command === 'native_resource_open'
    ? new Promise(resolve => { finish = () => resolve(invoke(command, args)) }) : invoke(command, args))
  const changing = nav.changeIndex(0); await flush()
  expect(finish).toBeDefined()
  nav.closeDialog(); finish(); await changing; await flush()
  expect(fixture.store.lightbox.sources[0].src).toBe('')
  expect(live.size).toBe(0)
  expect(status.loadError).toBe(false)
})

it('does not release another consumer of the current native source', async () => {
  mount(1)
  const item = fixture.store.lightbox.sources[0]
  const other = createNativeResourceOwner()
  const url = await other.url(item.path, item.size, '&v=version')
  await nav.changeIndex(0); await flush()
  expect(item.src).toBe(url)
  nav.closeDialog(); await flush()
  expect(live.size).toBe(1)
  expect(await other.url(item.path, item.size, '&v=version')).toBe(url)
  await other.dispose()
  expect(live.size).toBe(0)
})

it('retains an already opened popup source until both consumers are disposed', async () => {
  mount(1)
  const item = fixture.store.lightbox.sources[0]
  const popup = createNativeResourceOwner()
  item.src = await popup.url(item.path)
  await nav.changeIndex(0); await flush()
  await popup.dispose()
  expect(live.size).toBe(1)
  nav.closeDialog(); await flush()
  expect(live.size).toBe(0)
})

it('deduplicates looping neighbours and does not open audio solely for preloading', async () => {
  mount(2); loop.value = true
  fixture.store.lightbox.sources[1] = { ...source(1), path: '/other.mp3', name: 'other.mp3' }
  await nav.changeIndex(0); await flush()
  expect(live.size).toBe(1)
  expect(fixture.store.lightbox.sources[1].src).toBe('')
  fixture.store.lightbox.sources[1] = source(1)
  await nav.changeIndex(0); await flush()
  expect(live.size).toBe(2)
})

it('detaches ready video preload elements and deferred preload watchers when closing', async () => {
  mount(3)
  fixture.store.lightbox.sources[0] = { ...source(0), path: '/current.mp4', name: 'current.mp4' }
  fixture.store.lightbox.sources[1] = { ...source(1), path: '/next.mp4', name: 'next.mp4' }
  await nav.changeIndex(0); await flush()
  expect(live.size).toBe(0)
  expect(fixture.store.lightbox.sources[0].src).toMatch(/^http:/)
  status.loading = false; await flush()
  expect(live.size).toBe(1)
  expect(videos).toHaveLength(1)
  expect(videos[0].src).toMatch(/^http:/)
  videos[0].listeners.get('canplay')()
  nav.closeDialog(); await flush()
  expect(live.size).toBe(0)
  expect(videos[0].src).toBe('')
  expect(videos[0].listeners.size).toBe(0)
  expect(videos[0].load).toHaveBeenCalledTimes(2)
  expect(images.every(image => image.src === '' && image.onload === null)).toBe(true)
})

it('does not start deferred video neighbours after a close', async () => {
  mount(3)
  fixture.store.lightbox.sources[0] = { ...source(0), path: '/current.mp4', name: 'current.mp4' }
  await nav.changeIndex(0); await flush()
  nav.closeDialog(); status.loading = false; await flush()
  expect(live.size).toBe(0)
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_open')).toHaveLength(1)
})

it('retries a failed neighbour preload on the first navigation after recovery', async () => {
  mount(3)
  const neighbour = fixture.store.lightbox.sources[1]
  const invoke = fixture.invoke.getMockImplementation()!
  let unavailable = true
  fixture.invoke.mockImplementation((command: string, args: any) => command === 'native_resource_open' && args.path === neighbour.path && unavailable
    ? Promise.reject(new Error('temporarily unavailable')) : invoke(command, args))
  vi.spyOn(console, 'warn').mockImplementation(() => {})
  await nav.changeIndex(0); await flush()
  expect(neighbour.src).toBe('')
  unavailable = false
  await nav.changeIndex(1); await flush()
  expect(fixture.invoke.mock.calls.filter(([command, args]) => command === 'native_resource_open' && args.path === neighbour.path)).toHaveLength(2)
  expect(neighbour.src).toMatch(/^plain-resource:/)
  expect(status.loadError).toBe(false)
})

it('does not discard a newer binding when an evicted preload fails late', async () => {
  mount(20)
  const neighbour = fixture.store.lightbox.sources[1]
  const invoke = fixture.invoke.getMockImplementation()!
  let rejectOld: ((error: Error) => void) | undefined
  fixture.invoke.mockImplementation((command: string, args: any) => {
    if (command === 'native_resource_open' && args.path === neighbour.path && !rejectOld) {
      return new Promise((_, reject) => { rejectOld = reject })
    }
    return invoke(command, args)
  })
  vi.spyOn(console, 'warn').mockImplementation(() => {})
  await nav.changeIndex(0); await flush()
  expect(rejectOld).toBeDefined()
  await nav.changeIndex(10); await flush()
  await nav.changeIndex(0); await flush()
  const renewedUrl = neighbour.src
  expect(renewedUrl).toMatch(/^plain-resource:/)
  rejectOld!(new Error('old request failed'))
  await flush()
  await nav.changeIndex(1); await flush()
  expect(neighbour.src).toBe(renewedUrl)
  expect(status.loadError).toBe(false)
  expect(fixture.invoke.mock.calls.filter(([command, args]) => command === 'native_resource_open' && args.path === neighbour.path)).toHaveLength(2)
})
