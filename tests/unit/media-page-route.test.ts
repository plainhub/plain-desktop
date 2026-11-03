import { afterEach, expect, it, vi } from 'vitest'
import { createRenderer, h, KeepAlive, nextTick, reactive, ref } from 'vue'
const state = vi.hoisted(() => ({ route: {} as any }))
vi.mock('vue-router', () => ({ useRoute: () => state.route }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({ pageSize: 50 }) }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({}), urlTokenKey: ref(null) }) }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { subscribe: () => vi.fn() } }))
vi.mock('@/hooks/media', () => ({
  useBucketsTags: () => ({ tags: ref([]), buckets: ref([]), fetch: vi.fn() }),
  useDeleteItems: () => ({}), useBuckets: () => ({ view: vi.fn() }),
}))
vi.mock('@/hooks/list', () => ({ useSelectable: () => ({ total: ref(0), clearSelection: vi.fn() }), selectAllQuery: vi.fn() }))
vi.mock('@/hooks/files', () => ({ useDownload: () => ({ downloadFile: vi.fn() }), useDownloadItems: () => ({ downloadItems: vi.fn() }) }))
vi.mock('@/hooks/media-page-upload', () => ({ useMediaPageUpload: () => ({}) }))
vi.mock('@/hooks/media-trash', () => ({ useMediaRestore: () => ({}), useMediaTrash: () => ({}) }))
vi.mock('@/hooks/key-events', () => ({ useKeyEvents: () => ({ keyDown: vi.fn(), keyUp: vi.fn() }) }))
vi.mock('@/plugins/router', () => ({ replacePath: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { on: vi.fn(), off: vi.fn() } }))
vi.mock('@/lib/file', () => ({ getDirFromPath: vi.fn() }))
vi.mock('@/lib/format', () => ({ generateDownloadFileName: vi.fn() }))
vi.mock('@/lib/feature', () => ({ hasMediaTrash: () => false }))
vi.mock('@/lib/media/library-updates', () => ({ subscribeMediaLibraryUpdates: () => vi.fn() }))
import { useMediaPage, type MediaPageOptions } from '@/hooks/media-page'
import { encodeBase64 } from '@/lib/strutil'

afterEach(() => vi.unstubAllGlobals())
it.each(['audios', 'images', 'videos', 'docs'])('%s does not read a different page query before deactivation', async (routePath) => {
  vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() })
  state.route = reactive({ path: `/${routePath}`, fullPath: `/${routePath}`, query: {} as Record<string, string> })
  const requests: string[] = []
  let page: ReturnType<typeof useMediaPage>
  const renderer = createRenderer<any, any>({
    insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
    setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
  })
  const Page = { setup() {
    page = useMediaPage({ routePath, items: ref([]), sortByRef: ref('DATE_DESC'), fileFilter: () => true,
      getScrollMode: () => false, doFetch: () => requests.push(page?.q.value ?? '') } as MediaPageOptions)
    return () => null
  } }
  const app = renderer.createApp({ render: () => h(KeepAlive, null, () => h(Page)) })
  app.provide('isPhone', ref(false)); app.mount({})
  try {
    expect(requests).toEqual(['']); requests.length = 0
    const filesQuery = 'parent:/mnt/usb2 type:USB_STORAGE root_path:/mnt/usb2'
    Object.assign(state.route, { path: '/files', fullPath: '/files?q=directory', query: { q: encodeBase64(filesQuery) } })
    await nextTick()
    expect(requests).toEqual([])
    expect(page!.q.value).toBe('')
    Object.assign(state.route, { path: `/${routePath}`, fullPath: `/${routePath}?q=search`, query: { q: encodeBase64('text:music') } })
    await nextTick()
    expect(requests).toEqual(['text:music'])
  } finally { app.unmount() }
})
