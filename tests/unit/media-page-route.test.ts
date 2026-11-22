import { afterEach, expect, it, vi } from 'vitest'
import { createRenderer, h, KeepAlive, nextTick, reactive, ref } from 'vue'
const state = vi.hoisted(() => ({
  route: { path: '', fullPath: '', query: {} },
  subscriptions: new Map<string, (value: { types: string[] }) => void>(),
  attached: new Set<() => void>(),
  gqlFetch: vi.fn(),
}))
vi.mock('vue-router', () => ({ useRoute: () => state.route }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({ pageSize: 50 }) }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({}), urlTokenKey: ref(null) }) }))
vi.mock('@/lib/api/subscription-client', () => ({
  graphqlSubscriptions: {
    subscribe: (field: string, callback: (value: { types: string[] }) => void) => {
      state.subscriptions.set(field, callback)
      return () => state.subscriptions.delete(field)
    },
    onAttached: (callback: () => void) => {
      state.attached.add(callback)
      return () => state.attached.delete(callback)
    },
  },
}))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: state.gqlFetch }))
vi.mock('@/hooks/media', () => ({
  useBucketsTags: () => ({ tags: ref([]), buckets: ref([]), fetch: vi.fn() }),
  useDeleteMediaItem: () => vi.fn(), useBuckets: () => ({ view: vi.fn() }),
}))
vi.mock('@/hooks/files', () => ({ useDownload: () => ({ downloadFile: vi.fn() }), useDownloadItems: () => ({ downloadItems: vi.fn() }) }))
vi.mock('@/hooks/media-page-upload', () => ({ useMediaPageUpload: () => ({}) }))
vi.mock('@/hooks/media-trash', () => ({ useMediaRestore: () => ({}), useMediaTrash: () => ({}) }))
vi.mock('@/hooks/key-events', () => ({ useKeyEvents: () => ({ keyDown: vi.fn(), keyUp: vi.fn() }) }))
vi.mock('@/plugins/router', () => ({ replacePath: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/lib/api/mutation', () => ({
  deleteMediaItemsGQL: 'deleteMediaItems',
  initMutation: () => ({ mutate: vi.fn(), loading: ref(false) }),
}))
vi.mock('@/lib/file', () => ({ getDirFromPath: vi.fn() }))
vi.mock('@/lib/format', () => ({ generateDownloadFileName: vi.fn() }))
vi.mock('@/lib/feature', () => ({ hasMediaTrash: () => false }))
import { useMediaPage, type MediaPageOptions } from '@/hooks/media-page'
import { initLazyQuery } from '@/lib/api/query'
import { encodeBase64 } from '@/lib/strutil'
import { DataType } from '@/lib/data'
import emitter from '@/plugins/eventbus'

afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers() })

function mountMediaPage(dataType: MediaPageOptions['dataType'], routePath: string) {
  vi.useFakeTimers()
  vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() })
  state.route = reactive({ path: `/${routePath}`, fullPath: `/${routePath}`, query: {} })
  const first = { id: 'first' }
  const second = { id: 'second' }
  const items = ref([first, second])
  const pending: ((response: { data: { id: string }[] }) => void)[] = []
  state.gqlFetch.mockImplementation(() => new Promise((resolve) => pending.push(resolve)))
  let page!: ReturnType<typeof useMediaPage>
  const active = ref(true)
  const renderer = createRenderer<object, object>({
    insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
    setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
  })
  const doFetch = vi.fn((options) => fetch(undefined, options))
  let fetch: ReturnType<typeof initLazyQuery>['fetch']
  const Page = { setup() {
    page = useMediaPage({
      routePath, dataType, items, sortByRef: ref('UPDATED_AT_DESC'), fileFilter: () => true,
      getScrollMode: () => false, doFetch, downloadName: routePath, uploadModalId: routePath, uploadStorageKey: routePath,
    })
    const query = initLazyQuery({
      document: 'fixture',
      handle: (data: { id: string }[], error) => { if (!error) items.value = data },
    })
    fetch = query.fetch
    return () => null
  } }
  const app = renderer.createApp({ render: () => h(KeepAlive, null, () => active.value ? h(Page) : null) })
  app.provide('isPhone', ref(false))
  app.mount({})
  const push = (type = dataType) => state.subscriptions.get('mediaUpdated')?.({ types: [type] })
  const respond = async (request: number, data: { id: string }[]) => {
    pending[request]({ data })
    await vi.advanceTimersByTimeAsync(0)
  }
  return { app, active, page, first, second, doFetch, push, respond, pending }
}

const mediaRoutes = [
  [DataType.IMAGE, 'images'], [DataType.VIDEO, 'videos'], [DataType.AUDIO, 'audios'], [DataType.DOC, 'docs'],
] as const

it.each(mediaRoutes)('%s keeps surviving selections while the catalog refreshes', async (dataType, routePath) => {
  const { app, page, first, second, doFetch, push, respond } = mountMediaPage(dataType, routePath)
  try {
    await respond(0, [first, second])
    page.selectAll()
    page.deleteItems()
    push()
    await vi.advanceTimersByTimeAsync(2000)
    expect(doFetch).toHaveBeenLastCalledWith({ force: true, latest: true })
    expect(page.getQuery()).toBe('ids:first,second')
    expect(page.confirmingDelete.value).toBe(true)

    await respond(1, [{ id: 'new' }, { ...second }])
    expect(page.getQuery()).toBe('ids:second')
    expect(page.checked.value).toBe(true)
    expect(page.allChecked.value).toBe(false)
    expect(page.confirmingDelete.value).toBe(false)

    push()
    await vi.advanceTimersByTimeAsync(2000)
    await respond(2, [first])
    expect(page.checked.value).toBe(false)
  } finally { app.unmount() }
})

it('keeps explicit targets when a catalog push invalidates a query-wide selection', async () => {
  const { app, page, push } = mountMediaPage(DataType.IMAGE, 'images')
  try {
    page.selectAll()
    page.total.value = 10
    page.selectRealAll()
    page.deleteItems()
    expect(page.confirmingDelete.value).toBe(true)
    push()
    await vi.advanceTimersByTimeAsync(2000)
    expect(page.getQuery()).toBe('ids:first,second')
    expect(page.confirmingDelete.value).toBe(false)
  } finally { app.unmount() }
})

it('refreshes a completed upload without clearing selections or making duplicate requests', async () => {
  const { app, page, doFetch } = mountMediaPage(DataType.IMAGE, 'images')
  try {
    page.selectAll()
    emitter.emit('upload_task_done', {
      status: 'done', fileName: 'fixture.png', id: 'upload', dir: '/fixtures',
      file: new File([], 'fixture.png'), uploadedSize: 0, error: '',
    })
    await vi.advanceTimersByTimeAsync(2000)
    expect(page.getQuery()).toBe('ids:first,second')
    expect(doFetch).toHaveBeenCalledTimes(2)
  } finally { app.unmount() }
})

it('removes only the affected selection on a targeted media deletion', async () => {
  const { app, page, first, second, respond } = mountMediaPage(DataType.IMAGE, 'images')
  try {
    await respond(0, [first, second])
    page.selectAll()
    emitter.emit('media_items_actioned', { type: DataType.IMAGE, action: 'DELETE', id: first.id, query: 'ids:first' })
    expect(page.getQuery()).toBe('ids:second')
  } finally { app.unmount() }
})

it('retains the selection on reconnect and ignores a reply from before deactivation', async () => {
  const { app, active, page, first, second, pending, push, respond } = mountMediaPage(DataType.IMAGE, 'images')
  try {
    await respond(0, [first, second])
    page.toggleSelect({ shiftKey: false } as MouseEvent, second, 1)
    push()
    await vi.advanceTimersByTimeAsync(2000)
    expect(pending).toHaveLength(2)
    active.value = false
    await nextTick()
    state.attached.forEach((resnapshot) => resnapshot())
    expect(pending).toHaveLength(2)
    active.value = true
    await nextTick()
    await respond(2, [second, first])
    await respond(1, [])
    expect(page.getQuery()).toBe('ids:second')
    state.attached.forEach((resnapshot) => resnapshot())
    expect(pending).toHaveLength(4)
    await respond(3, [second, first])
    expect(page.getQuery()).toBe('ids:second')
  } finally { app.unmount() }
})

it('cancels pending refresh on leave and refreshes again on return', async () => {
  const { app, active, doFetch, push } = mountMediaPage(DataType.IMAGE, 'images')
  try {
    push(DataType.VIDEO)
    await vi.advanceTimersByTimeAsync(2000)
    expect(doFetch).toHaveBeenCalledTimes(1)
    push()
    active.value = false
    await nextTick()
    await vi.advanceTimersByTimeAsync(10000)
    expect(doFetch).toHaveBeenCalledTimes(1)
    active.value = true
    await nextTick()
    expect(doFetch).toHaveBeenCalledTimes(2)
    push()
    await vi.advanceTimersByTimeAsync(2000)
    expect(doFetch).toHaveBeenCalledTimes(3)
  } finally { app.unmount() }
})

it.each(mediaRoutes)('%s does not read a different page query before deactivation', async (dataType, routePath) => {
  const { app, page, doFetch } = mountMediaPage(dataType, routePath)
  try {
    expect(doFetch).toHaveBeenCalledTimes(1)
    Object.assign(state.route, { path: '/files', fullPath: '/files?q=directory', query: { q: encodeBase64('parent:/media') } })
    await nextTick()
    expect(doFetch).toHaveBeenCalledTimes(1)
    expect(page.q.value).toBe('')
    Object.assign(state.route, { path: `/${routePath}`, fullPath: `/${routePath}?q=search`, query: { q: encodeBase64('text:music') } })
    await nextTick()
    expect(doFetch).toHaveBeenCalledTimes(2)
    expect(page.q.value).toBe('text:music')
  } finally { app.unmount() }
})
