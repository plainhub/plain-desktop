import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createRenderer, reactive } from 'vue'

const mock = vi.hoisted(() => ({ fetch: vi.fn(), refresh: null as null | (() => Promise<void>) }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: mock.fetch }))
vi.mock('@/lib/api/mutation', () => ({ setPrefGQL: 'setPref' }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe: (_: string, refresh: () => Promise<void>) => { mock.refresh = refresh; return vi.fn() },
  onAttached: () => vi.fn(),
} }))
vi.mock('@/lib/prefs/core', () => ({ applySnapshot: vi.fn(), preferenceKeys: ['desktop_home_features', 'desktop_rail_features', 'desktop_upload_dirs'], preferencesQuery: 'prefs' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/prefs/appearance', () => ({ apply: vi.fn() }))
vi.mock('@/stores/main', () => ({ DEFAULT_RAIL_FEATURES: ['FILES', 'CHAT'] }))
vi.mock('@/views/home/features', () => ({ DEFAULT_HOME_FEATURES: ['FILES'] }))
import { useRemotePrefs } from '@/hooks/remote-prefs'
import type { useMainStore } from '@/stores/main'

let unmount: (() => void) | undefined
beforeEach(() => {
  vi.useFakeTimers()
  vi.stubGlobal('__IS_TAURI__', false)
  mock.fetch.mockReset()
})
afterEach(() => { unmount?.(); vi.useRealTimers(); vi.unstubAllGlobals() })

function mount() {
  const store = reactive({ railFeatures: ['FILES', 'CHAT'], homeFeatures: ['FILES'], uploadDirs: {} as Record<string, string> })
  const renderer = createRenderer({ insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}), setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn() })
  const app = renderer.createApp({ setup() {
    useRemotePrefs(store as ReturnType<typeof useMainStore>)
    return () => null
  } })
  app.mount({})
  unmount = () => app.unmount()
  return store
}

it('loads sidebar preferences and refreshes changes without writing them back', async () => {
  mock.fetch.mockResolvedValue({ data: { prefs: { desktop_rail_features: ['CHAT', 'FILES'], desktop_home_features: [] } } })
  const store = mount()
  await vi.advanceTimersByTimeAsync(500)
  expect(store.railFeatures).toEqual(['CHAT', 'FILES'])
  expect(store.homeFeatures).toEqual([])
  expect(mock.fetch).toHaveBeenCalledWith('prefs', { keys: ['desktop_home_features', 'desktop_rail_features', 'desktop_upload_dirs'] }, { fresh: true })
  mock.fetch.mockResolvedValue({ data: { prefs: { desktop_rail_features: [] } } })
  await mock.refresh!()
  await vi.advanceTimersByTimeAsync(500)
  expect(store.railFeatures).toEqual([])
  expect(store.homeFeatures).toEqual(['FILES'])
  expect(mock.fetch.mock.calls.every(([query]) => query === 'prefs')).toBe(true)
})

it('saves sidebar and home changes independently, including empty lists', async () => {
  mock.fetch.mockResolvedValue({ data: { prefs: {} } })
  const store = mount()
  await vi.advanceTimersByTimeAsync(0)
  mock.fetch.mockClear().mockResolvedValue({ data: { setPref: true } })
  store.railFeatures = ['AUDIO', 'CHAT']
  await vi.advanceTimersByTimeAsync(500)
  expect(mock.fetch.mock.calls).toEqual([
    ['setPref', { key: 'desktop_rail_features', value: ['AUDIO', 'CHAT'] }, { dedupe: false }],
  ])
  mock.fetch.mockClear()
  store.railFeatures = []
  store.homeFeatures = ['NOTES']
  await vi.advanceTimersByTimeAsync(500)
  expect(mock.fetch.mock.calls).toEqual([
    ['setPref', { key: 'desktop_rail_features', value: [] }, { dedupe: false }],
    ['setPref', { key: 'desktop_home_features', value: ['NOTES'] }, { dedupe: false }],
  ])
})

it('restores defaults when sidebar preferences are deleted', async () => {
  mock.fetch.mockResolvedValue({ data: { prefs: { desktop_rail_features: ['CHAT'] } } })
  const store = mount()
  await vi.advanceTimersByTimeAsync(0)
  mock.fetch.mockResolvedValue({ data: { prefs: {} } })
  await mock.refresh!()
  expect(store.railFeatures).toEqual(['FILES', 'CHAT'])
})

it('loads and saves upload directories through prefs, including nested changes and deletion', async () => {
  mock.fetch.mockResolvedValue({ data: { prefs: { desktop_upload_dirs: { images: '/pictures' } } } })
  const store = mount()
  await vi.advanceTimersByTimeAsync(500)
  expect(store.uploadDirs).toEqual({ images: '/pictures' })
  expect(mock.fetch.mock.calls.every(([query]) => query === 'prefs')).toBe(true)
  mock.fetch.mockClear().mockResolvedValue({ data: { setPref: true } })
  store.uploadDirs.images = '/photos'
  store.uploadDirs.videos = '/movies'
  await vi.advanceTimersByTimeAsync(500)
  expect(mock.fetch.mock.calls).toEqual([
    ['setPref', { key: 'desktop_upload_dirs', value: { images: '/photos', videos: '/movies' } }, { dedupe: false }],
  ])
  mock.fetch.mockResolvedValue({ data: { prefs: {} } })
  await mock.refresh!()
  expect(store.uploadDirs).toEqual({})
  mock.fetch.mockClear()
  await vi.advanceTimersByTimeAsync(500)
  expect(mock.fetch).not.toHaveBeenCalled()
})

it.each([[], null, 'invalid', { images: 42 }])('rejects invalid upload directory prefs %j', async value => {
  mock.fetch.mockResolvedValue({ data: { prefs: { desktop_upload_dirs: value } } })
  const store = mount()
  await vi.advanceTimersByTimeAsync(0)
  expect(store.uploadDirs).toEqual({})
})
