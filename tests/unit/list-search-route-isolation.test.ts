import { afterEach, expect, it, vi } from 'vitest'
import { h, KeepAlive, nextTick, onActivated, onDeactivated, reactive, ref } from 'vue'
import { renderView } from '../helpers/vue-node-renderer'
import toast from '@/components/toaster'
import { useTags } from '@/hooks/tags'
import { encodeBase64 } from '@/lib/strutil'

const state = vi.hoisted(() => ({ failure: false, route: {} as any, requests: [] as Array<{ domain: string; query: string }> }))
vi.mock('vue-router', () => ({ useRoute: () => state.route }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({ pageSize: 50, appSortBy: ref('NAME_ASC') }) }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({}), feedsSyncing: ref(false) }) }))
vi.mock('@/plugins/router', () => ({ replacePath: vi.fn() }))
vi.mock('@/lib/api/file', () => ({ getFileUrlByPath: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { on: vi.fn(), off: vi.fn() } }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { subscribe: () => vi.fn() } }))
vi.mock('@/lib/api/query', () => ({
  tagsGQL: 'tags', notesGQL: 'notes', contactsGQL: 'contacts', callsGQL: 'calls', packagesGQL: 'apps', packageStatusesGQL: 'packageStatuses', contactSourcesGQL: 'sources', feedsTagsGQL: 'feedsTags',
  initQuery: () => ({}),
  initLazyQuery: ({ document, variables, handle }: any) => ({ loading: ref(false), fetch: () => {
    if (state.failure && ['tags', 'calls'].includes(document)) handle(null, 'NO_PERMISSION')
    if (typeof variables === 'function') state.requests.push({ domain: document, query: variables().query })
  } }),
}))
vi.mock('@/lib/api/mutation', () => ({ deleteContactsGQL: 'deleteContacts', deleteCallsGQL: 'deleteCalls' }))
vi.mock('@/hooks/list', () => ({ useSelectable: () => ({ total: ref(0) }), useDelete: () => ({}) }))
vi.mock('@/hooks/key-events', () => ({ useKeyEvents: () => ({ keyDown: vi.fn(), keyUp: vi.fn() }) }))
vi.mock('@/hooks/feed-entries', () => ({ useList: (_items: unknown, query: { value: string }) => ({
  page: ref(1), loading: ref(false), noMore: ref(false),
  fetch: () => state.requests.push({ domain: 'feeds', query: query.value }),
}) }))
import { useNotesData } from '@/views/notes/hooks/useNotesData'
import { useFeedEntriesData } from '@/views/feeds/hooks/useFeedEntriesData'
import { useAppsData } from '@/views/apps/hooks/useAppsData'
import { useCallsData } from '@/views/calls/hooks/useCallsData'
import { useContactsData } from '@/views/contacts/hooks/useContactsData'

afterEach(() => vi.unstubAllGlobals())
it.each([
  ['notes', () => useNotesData(vi.fn())],
  ['feeds', () => useFeedEntriesData(vi.fn())],
  ['contacts', () => useContactsData()],
  ['calls', () => useCallsData()],
  ['apps', () => {
    const data = useAppsData()
    onActivated(() => { data.isActive.value = true; data.applyRouteQuery() })
    onDeactivated(() => { data.isActive.value = false })
    return data
  }],
] as const)('%s keeps its query when leaving for Files before KeepAlive deactivation', async (domain, useData) => {
  vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() })
  vi.stubGlobal('document', { getElementsByClassName: () => [] })
  state.requests = []
  const routeTo = (path: string, query: string) => Object.assign(state.route, { path, fullPath: `${path}?q=${encodeBase64(query)}`, params: {}, query: { q: encodeBase64(query) } })
  state.route = reactive({})
  routeTo(`/${domain}`, 'tag_id:synthetic text:Original')
  const visible = ref(true)
  let data!: ReturnType<typeof useNotesData | typeof useFeedEntriesData | typeof useContactsData | typeof useCallsData | typeof useAppsData>
  const Page = { setup() { data = useData(); return () => h('section') } }
  const view = renderView({ setup: () => () => h(KeepAlive, null, { default: () => visible.value ? h(Page) : h('aside') }) })
  try {
    expect(state.requests).toEqual([{ domain, query: 'tag_id:synthetic text:Original' }])
    state.requests = []
    routeTo('/files', 'parent:/synthetic type:USB_STORAGE root_path:/synthetic')
    visible.value = false
    await nextTick()
    expect(state.requests).toEqual([])
    expect(data.q.value).toBe('tag_id:synthetic text:Original')
    routeTo('/files', 'parent:/synthetic/other')
    await nextTick()
    expect(state.requests).toEqual([])
    routeTo(`/${domain}`, 'text:Returned')
    visible.value = true
    await nextTick()
    expect(state.requests).toEqual([{ domain, query: 'text:Returned' }])
    state.requests = []
    routeTo(`/${domain}`, 'text:Updated')
    await nextTick()
    expect(state.requests).toEqual([{ domain, query: 'text:Updated' }])
  } finally { view.unmount() }
})

it('shows denied calls in the list and clears records from the previous load', () => {
  vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() })
  state.route = reactive({ path: '/calls', fullPath: '/calls', query: {} })
  let calls!: ReturnType<typeof useCallsData>
  const view = renderView({ setup() { calls = useCallsData(); return () => h('section') } })
  try {
    calls.items.value = [{ id: '1', name: '', number: '', durationSec: 0, accountId: '', startedAt: '', photo: '', type: 'MISSED', tags: [] }]
    calls.total.value = 1
    state.failure = true
    vi.mocked(toast).mockClear()
    calls.fetch()
    expect(calls.items.value).toEqual([])
    expect(toast).not.toHaveBeenCalled()
    expect(calls.total.value).toBe(0)
    expect(calls.listError.value).toBe('NO_PERMISSION')
  } finally {
    state.failure = false
    view.unmount()
  }
})

it('clears the calls tag filter without a second permission notification', () => {
  let filter!: ReturnType<typeof useTags>
  const view = renderView({ setup() { filter = useTags('CALL'); return () => h('aside') } })
  try {
    filter.tags.value = [{ id: '1', name: 'Example', count: 1 }]
    state.failure = true
    vi.mocked(toast).mockClear()
    filter.fetch()
    expect(filter.tags.value).toEqual([])
    expect(toast).not.toHaveBeenCalled()
  } finally {
    state.failure = false
    view.unmount()
  }
})
