import { expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { renderView } from '../helpers/vue-node-renderer'

const state = vi.hoisted(() => ({ queries: [] as any[], listeners: new Set<(value: any) => void>(), mutation: vi.fn(), toast: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/stores/main', () => ({ useMainStore: vi.fn() }))
vi.mock('@/stores/temp', () => ({ useTempStore: vi.fn() }))
vi.mock('@/hooks/contact-picker', () => ({ useContactPicker: vi.fn() }))
vi.mock('@/lib/api/query', () => ({
  initQuery: (query: any) => { state.queries.push(query); return { refetch: vi.fn() } },
  fileIndexScanProgressGQL: 'progress', homeStatsGQL: vi.fn(), simsGQL: '',
}))
vi.mock('@/lib/api/mutation', () => ({
  initMutation: ({ document }: any) => ({ mutate: (variables: unknown) => state.mutation(document, variables), loading: ref(false) }),
  callGQL: '', pauseFileIndexGQL: 'pauseFileIndex', resumeFileIndexGQL: 'resumeFileIndex', stopFileIndexGQL: 'stopFileIndex',
}))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe: (_field: string, listener: (value: any) => void) => {
    state.listeners.add(listener)
    return () => state.listeners.delete(listener)
  },
} }))
vi.mock('@/components/toaster', () => ({ default: state.toast }))
import { useScanAction } from '@/views/home/home'

it('keeps each volume progress and every control scoped without success or failure toasts', async () => {
  state.queries.length = 0
  state.mutation.mockClear()
  let first!: ReturnType<typeof useScanAction>
  let second!: ReturnType<typeof useScanAction>
  const view = renderView({ setup() { first = useScanAction('first'); second = useScanAction('second'); return () => null } })
  try {
    expect(state.queries.map(query => query.variables)).toEqual([{ volumeId: 'first' }, { volumeId: 'second' }])
    for (const listener of state.listeners) listener({ volumeId: 'first', state: 'RUNNING', indexed: 1234, total: null, pending: null })
    expect(first.fileIndexScanProgress.value).toMatchObject({ state: 'RUNNING', indexed: 1234, total: null })
    expect(second.fileIndexScanProgress.value.state).toBe('IDLE')
    for (const scanState of ['QUEUED', 'PAUSED', 'FAILED', 'STOPPED', 'IDLE']) {
      for (const listener of state.listeners) listener({ volumeId: 'second', state: scanState, indexed: 0, total: null, error: 'generated failure' })
      expect(second.fileIndexScanProgress.value.state).toBe(scanState)
      expect(first.fileIndexScanProgress.value.state).toBe('RUNNING')
    }
    await first.pause()
    await second.resume()
    await second.stop()
    await first.rebuild()
    expect(state.mutation.mock.calls.slice(0, 3)).toEqual([
      ['pauseFileIndex', { volumeId: 'first' }], ['resumeFileIndex', { volumeId: 'second' }], ['stopFileIndex', { volumeId: 'second' }],
    ])
    expect(state.mutation.mock.calls[3]).toEqual([expect.stringContaining('rebuildFileIndex'), { volumeId: 'first' }])
    expect(state.toast).not.toHaveBeenCalled()
  } finally { view.unmount() }
  expect(state.listeners.size).toBe(0)
})
