import { afterEach, expect, it, vi } from 'vitest'
import { createRenderer, nextTick, ref } from 'vue'

const state = vi.hoisted(() => ({ context: undefined as any, event: undefined as any }))
vi.mock('@/lib/api/query', () => ({ prefsGQL: 'prefs' }))
vi.mock('@/lib/api/gql-client', async () => {
  const { ref } = await import('vue')
  state.context = ref('first')
  return { gqlFetch: vi.fn(), getGraphqlContextKey: () => state.context.value }
})
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe: (_: string, callback: () => void) => { state.event = callback; return vi.fn() },
  onAttached: () => vi.fn(),
} }))
import { gqlFetch } from '@/lib/api/gql-client'
import { useFileIndexStatus } from '@/views/files/hooks/useFileIndexStatus'

let unmount: (() => void) | undefined
function mount(enabled: ReturnType<typeof ref<boolean>>) {
  const renderer = createRenderer<any, any>({
    insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
    setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
  })
  let result!: ReturnType<typeof useFileIndexStatus>
  const app = renderer.createApp({ setup() { result = useFileIndexStatus(enabled); return () => null } })
  app.mount({}); unmount = () => app.unmount(); return result
}
const response = (status: 'running' | 'completed') => ({ data: { prefs: { file_index_progress: { status, processedFiles: 5, sources: [], error: null } } } })
afterEach(() => { unmount?.(); vi.useRealTimers(); vi.mocked(gqlFetch).mockReset(); state.context.value = 'first' })

it('polls only an active indexed view and coalesces progress events', async () => {
  vi.useFakeTimers()
  vi.mocked(gqlFetch).mockResolvedValue(response('running') as any)
  const enabled = ref(false); mount(enabled)
  state.event(); await vi.advanceTimersByTimeAsync(5000)
  expect(gqlFetch).not.toHaveBeenCalled()
  enabled.value = true; await nextTick(); await Promise.resolve()
  expect(gqlFetch).toHaveBeenCalledTimes(1)
  state.event(); state.event(); await vi.advanceTimersByTimeAsync(5000)
  expect(gqlFetch).toHaveBeenCalledTimes(2)
  enabled.value = false; await nextTick(); await vi.advanceTimersByTimeAsync(10000)
  expect(gqlFetch).toHaveBeenCalledTimes(2)
})

it('discards a previous device response after switching context', async () => {
  let finish!: (value: any) => void
  vi.mocked(gqlFetch).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }) as any)
  vi.mocked(gqlFetch).mockResolvedValue(response('completed') as any)
  const result = mount(ref(true))
  state.context.value = 'second'; await nextTick(); await Promise.resolve()
  expect(result.progress.value?.status).toBe('completed')
  finish(response('running')); await Promise.resolve(); await Promise.resolve()
  expect(result.progress.value?.status).toBe('completed')
})
