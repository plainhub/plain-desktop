import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createRenderer, reactive, ref, h, KeepAlive, nextTick } from 'vue'
import { renderView } from '../helpers/vue-node-renderer'

const state = vi.hoisted(() => ({
  query: undefined as any,
  progress: undefined as any,
  refetch: vi.fn(),
  stop: vi.fn(),
  store: undefined as any,
}))
vi.mock('@/lib/api/query', () => ({
  homeStatsGQL: vi.fn(), simsGQL: '', fileIndexScanProgressGQL: '',
  initQuery: (query: unknown) => { state.query = query; return { refetch: state.refetch } },
}))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe: (field: string, callback: unknown) => {
    expect(field).toBe('fileIndexScanProgressUpdated')
    state.progress = callback
    return state.stop
  },
} }))
vi.mock('@/lib/api/file-index', () => ({ rebuildFileIndexGQL: '' }))
vi.mock('@/lib/api/mutation', () => ({
  callGQL: '', initMutation: vi.fn(), pauseFileIndexGQL: '', resumeFileIndexGQL: '', stopFileIndexGQL: '',
}))
vi.mock('pinia', async () => ({ storeToRefs: (await import('vue')).toRefs }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => state.store }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('@/hooks/contact-picker', () => ({ useContactPicker: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
import { useHomeData, useScanAction } from '@/views/home/home'

let unmount: () => void
let home: ReturnType<typeof useHomeData>
beforeEach(() => {
  vi.clearAllMocks()
  state.store = reactive({ counter: { images: 0, docs: 0, total: 0, free: 0 }, app: { capabilities: ['MEDIA_SCAN'] } })
  const renderer = createRenderer<any, any>({
    insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
    setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
  })
  const app = renderer.createApp({ setup() { home = useHomeData(); return () => null } })
  app.mount({})
  unmount = () => app.unmount()
})
afterEach(() => unmount())

it('refreshes Home counts and storage from a fresh read when indexing completes', async () => {
  const mounts = [{ mountPoint: '/', totalBytes: 1000, freeBytes: 400 }]
  state.refetch.mockImplementationOnce(() => state.query.handle({ imageCount: 12, docCount: 3, mounts }, ''))
  await state.progress({ state: 'IDLE', indexed: 15, total: 15 })
  expect(state.refetch).toHaveBeenCalledExactlyOnceWith(undefined, true)
  expect(state.store.counter).toMatchObject({ images: 12, docs: 3, total: 1000, free: 400 })
  expect(home.mounts.value).toEqual(mounts)
})

it('keeps the last home snapshot silently when a background refresh fails', async () => {
  const mounts = [{ mountPoint: '/', totalBytes: 1000, freeBytes: 400 }]
  state.query.handle({ imageCount: 12, mounts }, '')
  state.refetch.mockImplementationOnce(() => state.query.handle(null, 'Permission denied'))
  await state.progress({ state: 'IDLE' })
  expect(home.mounts.value).toEqual(mounts)
  expect(state.store.counter.images).toBe(12)
  expect((await import('@/components/toaster')).default).not.toHaveBeenCalled()
})

it.each(['RUNNING', 'PAUSED', 'STOPPED', 'FAILED'])('does not refresh on %s progress', async (scanState) => {
  await state.progress({ state: scanState, indexed: 5, total: 10 })
  expect(state.refetch).not.toHaveBeenCalled()
})

it('refreshes even when the completed scan indexed no files', async () => {
  await state.progress({ state: 'IDLE', indexed: 0, total: 0 })
  expect(state.refetch).toHaveBeenCalledExactlyOnceWith(undefined, true)
})

it('unsubscribes when Home unmounts', () => {
  unmount()
  expect(state.stop).toHaveBeenCalledOnce()
})

it('stops completion refresh while cached and refreshes once when returning Home', async () => {
  unmount()
  const visible = ref(true)
  const Home = { setup() { useHomeData(); return () => h('div') } }
  const view = renderView({ render: () => h(KeepAlive, null, { default: () => visible.value ? h(Home) : null }) })
  unmount = view.unmount
  state.refetch.mockClear()
  expect(state.refetch).not.toHaveBeenCalled()
  visible.value = false
  await nextTick()
  await state.progress({ state: 'IDLE', indexed: 1, total: 1 })
  expect(state.refetch).not.toHaveBeenCalled()
  visible.value = true
  await nextTick()
  expect(state.refetch).toHaveBeenCalledExactlyOnceWith(undefined, true)
  await state.progress({ state: 'IDLE', indexed: 2, total: 2 })
  expect(state.refetch).toHaveBeenCalledTimes(2)
})

it('keeps active scan and failure feedback without a persistent stopped label', async () => {
  const { initMutation } = await import('@/lib/api/mutation')
  vi.mocked(initMutation).mockReturnValue({ mutate: vi.fn(), loading: ref(false) } as any)
  let action: ReturnType<typeof useScanAction> | undefined
  const view = renderView({ setup() { action = useScanAction('fixture-volume'); return () => null } })
  try {
    state.query.handle({ fileIndexScanProgress: { indexed: 2, total: 3, state: 'STOPPED' } }, '')
    expect(action!.fileIndexScanProgress.value.state).toBe('STOPPED')
    state.query.handle({ fileIndexScanProgress: { indexed: 0, total: null, state: 'RUNNING' } }, '')
    expect(action!.fileIndexScanProgress.value).toMatchObject({ state: 'RUNNING', total: null })
    state.query.handle({ fileIndexScanProgress: { indexed: 2, total: 3, state: 'PAUSED' } }, '')
    expect(action!.fileIndexScanProgress.value.state).toBe('PAUSED')
    state.query.handle({ fileIndexScanProgress: { indexed: 2, total: 3, state: 'FAILED', error: 'fixture failure' } }, '')
    expect(action!.fileIndexScanProgress.value.error).toBe('fixture failure')
  } finally { view.unmount() }
})
