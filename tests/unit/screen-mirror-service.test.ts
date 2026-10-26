import { ref } from 'vue'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ queries: [] as any[], fetches: [] as any[] }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (value: string) => value }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/plugins/tapphone', () => ({ default: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: vi.fn() } }))
vi.mock('@/components/modal', () => ({ openModal: vi.fn() }))
vi.mock('@/components/AccessibilityGuideModal.vue', () => ({ default: {} }))
vi.mock('@/lib/api/query', () => ({
  initLazyQuery: (params: any) => {
    const index = mock.queries.push(params) - 1
    return { loading: ref(false), fetch: (_vars: unknown, options: unknown) => mock.fetches.push({ index, options }) }
  },
  screenMirrorControlEnabledGQL: 'control', isScreenMirroringGQL: 'mirror',
}))
vi.mock('@/lib/api/mutation', () => ({
  initMutation: () => ({ mutate: vi.fn(), loading: ref(false), onDone: vi.fn(), onError: vi.fn() }),
  relaunchAppGQL: '', startScreenMirrorGQL: '', stopScreenMirrorGQL: '', updateScreenMirrorQualityGQL: '', requestScreenMirrorAudioGQL: '',
}))
import { useScreenMirrorService } from '../../src/views/screen-mirror/screen-mirror-service'

beforeEach(() => { mock.queries.length = 0; mock.fetches.length = 0; vi.useFakeTimers() })
afterEach(() => vi.useRealTimers())

it('ignores late mirror state after leaving and after a new activation', () => {
  const service = useScreenMirrorService()
  const connect = vi.fn()
  service.setPipeline(connect, vi.fn())
  service.fetchState()
  const old = mock.fetches[0].options.meta
  service.deactivate()
  mock.queries[0].handle({ isScreenMirroring: true }, '', { meta: old })
  expect(service.state.value).toBe('idle')
  service.fetchState()
  mock.queries[0].handle({ isScreenMirroring: true }, '', { meta: old })
  expect(connect).not.toHaveBeenCalled()
  mock.queries[0].handle({ isScreenMirroring: true }, '', { meta: mock.fetches[1].options.meta })
  expect(connect).toHaveBeenCalledOnce()
})

it('retries a media connection that drops before its first frame and cancels retry on exit', () => {
  const service = useScreenMirrorService()
  const connect = vi.fn()
  service.setPipeline(connect, vi.fn())
  service.fetchState()
  mock.queries[0].handle({ isScreenMirroring: true }, '', { meta: mock.fetches[0].options.meta })
  service.onDisconnected()
  vi.advanceTimersByTime(1500)
  expect(connect).toHaveBeenCalledTimes(2)
  service.onDisconnected()
  service.deactivate()
  vi.runAllTimers()
  expect(connect).toHaveBeenCalledTimes(2)
})
