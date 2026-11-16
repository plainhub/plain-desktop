import { afterEach, expect, it, vi } from 'vitest'
import { h, KeepAlive, nextTick, reactive, ref } from 'vue'
import { renderView } from '../helpers/vue-node-renderer'
import { encodeBase64 } from '@/lib/strutil'
const state = vi.hoisted(() => ({ route: {} as any, requests: [] as string[] }))
vi.mock('vue-router', () => ({ useRoute: () => state.route }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({}), urlTokenKey: ref(null) }) }))
vi.mock('@/stores/sms', () => ({ useSmsStore: () => ({
  conversations: ref([]), noMore: ref(false), conversationCount: 0,
  fetchConversations: (query: string) => { state.requests.push(query); return Promise.resolve() },
  fetchArchived: () => { state.requests.push('archived'); return Promise.resolve() }, fetchCounts: vi.fn(),
}) }))
vi.mock('@/hooks/contacts', () => ({ useContactName: () => ({ loadContacts: vi.fn() }) }))
vi.mock('@/hooks/list', () => ({ useSelectable: () => ({ total: ref(0) }) }))
vi.mock('@/hooks/sms-notification-refresh', () => ({ createSmsNotificationRefresh: () => ({ subscribe: vi.fn(), unsubscribe: vi.fn() }) }))
vi.mock('@/plugins/router', () => ({ replacePath: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { on: vi.fn(), off: vi.fn() } }))
vi.mock('@/components/modal', () => ({ openModal: vi.fn() }))
vi.mock('@/views/messages/SendSmsModal.vue', () => ({ default: {} }))
vi.mock('@/views/messages/ExportSmsModal.vue', () => ({ default: {} }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { subscribe: () => vi.fn() } }))
import { useMessagesSidebar } from '@/hooks/messages-sidebar'
afterEach(() => vi.unstubAllGlobals())
it('does not submit a Files query from the cached message sidebar', async () => {
  state.requests = []
  state.route = reactive({ path: '/messages', query: { q: encodeBase64('text:Original') } })
  const visible = ref(true)
  const Page = { setup() { useMessagesSidebar(); return () => h('section') } }
  const view = renderView({ setup: () => () => h(KeepAlive, null, { default: () => visible.value ? h(Page) : h('aside') }) })
  try {
    expect(state.requests).toEqual(['text:Original'])
    state.requests = []
    state.route.path = '/files'
    state.route.query.q = encodeBase64('parent:/synthetic root_path:/synthetic')
    visible.value = false
    await nextTick()
    expect(state.requests).toEqual([])
    state.route.path = '/messages'
    state.route.query.q = encodeBase64('text:Returned')
    visible.value = true
    await nextTick()
    expect(state.requests).toEqual(['text:Returned'])
    state.requests = []
    visible.value = false
    await nextTick()
    state.route.path = '/messages/archived'
    state.route.query.q = ''
    visible.value = true
    await nextTick()
    expect(state.requests).toEqual(['archived'])
  } finally { view.unmount() }
})
