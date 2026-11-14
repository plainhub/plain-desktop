import { afterEach, expect, it, vi } from 'vitest'
import { createRenderer, nextTick, reactive, ref } from 'vue'

const fixture = vi.hoisted(() => ({ store: null as any, remote: null as any }))
vi.mock('@/stores/main', () => ({ useMainStore: () => fixture.store }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref(null), urlTokenKey: ref(null) }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('vue-router', () => ({ useRouter: () => ({ currentRoute: ref({ fullPath: '/', path: '/', query: {}, matched: [] }) }) }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/api/api', () => ({ setUrlToken: vi.fn() }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { subscribe: () => vi.fn() } }))
vi.mock('@/lib/api/query', () => ({ appGQL: '', initQuery: () => ({ refetch: vi.fn() }) }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { on: vi.fn(), off: vi.fn() } }))
vi.mock('@/hooks/sidebar', () => ({ useRightSidebarResize: () => ({ resizeWidth: 0 }) }))
vi.mock('@/lib/device/current', () => ({ getMainStateKey: () => 'main_state:test' }))
vi.mock('@/lib/upload/batch-progress', () => ({ hasActiveUploadBatches: () => false }))
vi.mock('@/hooks/audio-playlist-store', () => ({ useAudioPlaylistStore: () => ({ refetch: vi.fn() }) }))
vi.mock('@/components/modal', () => ({ openModal: vi.fn() }))
vi.mock('@/views/chat/PairingRequestModal.vue', () => ({ default: {} }))
vi.mock('@/lib/media/library-updates', () => ({ subscribeMediaLibraryUpdates: () => vi.fn() }))
vi.mock('@/hooks/remote-prefs', () => ({ useRemotePrefs: () => fixture.remote }))
import { useMainView } from '@/hooks/main-view'

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals() })

it.each([false, true])('saves the latest view and sort synchronously on pagehide with remote prefs %s', async remote => {
  vi.useFakeTimers()
  const values = new Map<string, string>()
  vi.stubGlobal('window', new EventTarget())
  vi.stubGlobal('localStorage', { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => values.set(key, value) })
  fixture.store = { $state: reactive({ imagesCardView: false, fileSortBy: 'NAME_ASC', homeFeatures: ['FILES'], chatTexts: { local: 'draft' }, searchHistory: { files: ['query'] } }) }
  fixture.remote = ref(remote)
  const renderer = createRenderer<any, any>({ insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}), setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn() })
  const app = renderer.createApp({ setup() { useMainView(); return () => null } })
  app.mount({})
  try {
    fixture.store.$state.imagesCardView = true
    fixture.store.$state.fileSortBy = 'NAME_DESC'
    await nextTick()
    expect(values.has('main_state:test')).toBe(false)
    window.dispatchEvent(new Event('pagehide'))
    expect(values.has('main_state:test')).toBe(true)
    const saved = JSON.parse(values.get('main_state:test')!)
    expect(saved).toEqual({ imagesCardView: true, fileSortBy: 'NAME_DESC', ...(remote ? {} : { homeFeatures: ['FILES'] }) })
  } finally { app.unmount() }
  values.clear()
  window.dispatchEvent(new Event('pagehide'))
  expect(values.size).toBe(0)
})

it.each([
  ...['DATE_DESC', 'DATE_ASC', 'TITLE_ASC', 1, null].map(value => [value, 'UPDATED_AT_DESC']),
  ...['UPDATED_AT_ASC', 'UPDATED_AT_DESC', 'SIZE_ASC', 'SIZE_DESC', 'NAME_ASC', 'NAME_DESC', 'TAKEN_AT_DESC'].map(value => [value, value]),
])('restores persisted image sort %s as %s before queries can use it', async (imageSortBy, expected) => {
  const values = new Map([['main_state:test', JSON.stringify({ imageSortBy, imagesCardView: true })]])
  vi.stubGlobal('window', new EventTarget())
  vi.stubGlobal('localStorage', { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => values.set(key, value) })
  fixture.store = { $state: reactive({ imageSortBy: 'UPDATED_AT_DESC', imagesCardView: false }) }
  fixture.remote = ref(false)
  const renderer = createRenderer<any, any>({ insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}), setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn() })
  const app = renderer.createApp({ setup() { useMainView(); return () => null } })
  app.mount({})
  try {
    expect(fixture.store.$state.imageSortBy).toBe(expected)
    expect(fixture.store.$state.imagesCardView).toBe(true)
  } finally { app.unmount() }
})
