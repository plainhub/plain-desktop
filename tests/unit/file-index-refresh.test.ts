import { afterEach, expect, it, vi } from 'vitest'
import { h, KeepAlive, ref } from 'vue'
import { clientTemplate, flush, renderView } from '../helpers/vue-node-renderer'

const state = vi.hoisted(() => ({
  fetch: vi.fn(),
  listeners: new Map<string, Set<(value: any) => void>>(),
  attached: new Set<() => void>(),
  modal: vi.fn(),
  progress: undefined as any,
}))
vi.mock('@/lib/api/gql-client', () => ({
  gqlFetch: state.fetch,
  GqlError: class extends Error {},
}))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe(field: string, listener: (value: any) => void) {
    const listeners = state.listeners.get(field) ?? new Set()
    state.listeners.set(field, listeners)
    listeners.add(listener)
    return () => listeners.delete(listener)
  },
  onAttached(listener: () => void) {
    state.attached.add(listener)
    return () => state.attached.delete(listener)
  },
} }))
vi.mock('@/components/modal', () => ({ pushModal: state.modal }))
vi.mock('@/views/home/file-index/FileIndexScopeDialog.vue', () => ({ default: {} }))
vi.mock('@/views/home/file-index/FileIndexVolumeCard.vue', () => ({ default: { render: () => h('div') } }))
vi.mock('@/views/home/home', () => ({ useScanAction: () => ({
  fileIndexScanProgress: state.progress = ref({ indexed: 0, total: 0, state: 'IDLE' }), scanActive: ref(false), percent: ref(0), stateLabel: ref(''),
  showPause: ref(false), showResume: ref(false), showStop: ref(false), showRebuild: ref(false),
  loading: ref(false), pause: vi.fn(), resume: vi.fn(), stop: vi.fn(), rebuild: vi.fn(),
}) }))
import FileIndexCard from '@/views/home/file-index/FileIndexCard.vue'
import FileIndexVolume from '@/views/home/file-index/FileIndexVolumeItem.vue'
clientTemplate(FileIndexCard, 'src/views/home/file-index/FileIndexCard.vue')
clientTemplate(FileIndexVolume, 'src/views/home/file-index/FileIndexVolumeItem.vue')

let unmount: (() => void) | undefined
afterEach(() => {
  unmount?.()
  state.listeners.clear()
  state.attached.clear()
  vi.clearAllMocks()
  vi.useRealTimers()
})

it('queries configuration on mount changes, reconnect and returning Home without media-driven refresh', async () => {
  vi.useFakeTimers()
  state.fetch.mockResolvedValue({ data: {
    fileIndexVolumes: [], fileIndexDefaults: { excludedDirs: [], excludedNames: [], excludedPackages: [], excludedUserLibraries: false },
  } })
  const visible = ref(true)
  const mounts = ref<any[]>([])
  const view = renderView({ render: () => h(KeepAlive, null, {
    default: () => visible.value ? h(FileIndexCard, { mounts: mounts.value }) : null,
  }) }, {}, { VIconButton: { render: () => null }, VTextButton: { render: () => null } })
  unmount = view.unmount
  await flush()
  expect(state.fetch).toHaveBeenCalledTimes(1)
  const mediaChanged = () => {
    for (const listener of state.listeners.get('mediaUpdated') ?? []) listener({})
  }
  mediaChanged()
  await vi.advanceTimersByTimeAsync(12000)
  expect(state.fetch).toHaveBeenCalledTimes(1)
  mounts.value = [{ mountPoint: '/' }]
  await flush()
  expect(state.fetch).toHaveBeenCalledTimes(2)
  mounts.value = [{ mountPoint: '/', freeBytes: 20 }]
  await flush()
  expect(state.fetch).toHaveBeenCalledTimes(2)
  for (const listener of state.attached) listener()
  await flush()
  expect(state.fetch).toHaveBeenCalledTimes(3)
  mediaChanged()
  visible.value = false
  await flush()
  mounts.value = [{ mountPoint: '/' }, { mountPoint: '/media/test' }]
  for (const listener of state.attached) listener()
  mediaChanged()
  await vi.advanceTimersByTimeAsync(12000)
  expect(state.fetch).toHaveBeenCalledTimes(3)
  visible.value = true
  await flush()
  expect(state.fetch).toHaveBeenCalledTimes(4)
  view.unmount()
  unmount = undefined
  expect(state.attached.size).toBe(0)
})

it('refreshes terminal scan snapshots including cancel completion and skips refresh while Home is cached', async () => {
  state.fetch.mockResolvedValue({ data: { fileIndexVolumes: [{ id: 'fixture', path: '/fixture', isAvailable: true, indexMode: 'BLOCKLIST', includedDirs: [], excludedDirs: [] }], fileIndexDefaults: {} } })
  const visible = ref(true)
  const view = renderView({ render: () => h(KeepAlive, null, {
    default: () => visible.value ? h(FileIndexCard, { mounts: [{ id: 'fixture', mountPoint: '/fixture' }] }) : null,
  }) }, {}, { VIconButton: { render: () => null }, VTextButton: { render: () => null } })
  unmount = view.unmount
  await flush()
  expect(state.fetch).toHaveBeenCalledTimes(1)
  state.progress.value = { ...state.progress.value, state: 'RUNNING' }
  await flush()
  expect(state.fetch).toHaveBeenCalledTimes(1)
  state.progress.value = { ...state.progress.value, state: 'STOPPED' }
  await flush()
  expect(state.fetch).toHaveBeenCalledTimes(2)
  state.progress.value.indexed = 10
  await flush()
  expect(state.fetch).toHaveBeenCalledTimes(2)
  state.progress.value = { ...state.progress.value, state: 'STOPPED' }
  await flush()
  expect(state.fetch).toHaveBeenCalledTimes(3)
  visible.value = false
  await flush()
  state.progress.value = { ...state.progress.value, state: 'RUNNING' }
  await flush()
  state.progress.value = { ...state.progress.value, state: 'IDLE' }
  await flush()
  expect(state.fetch).toHaveBeenCalledTimes(3)
  visible.value = true
  await flush()
  expect(state.fetch).toHaveBeenCalledTimes(4)
})
