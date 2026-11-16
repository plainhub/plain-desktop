import { defineComponent, h, KeepAlive, ref } from 'vue'
import { afterEach, expect, it, vi } from 'vitest'
import { useFeedEntriesData } from '@/views/feeds/hooks/useFeedEntriesData'
import { content, flush, renderView } from '../helpers/vue-node-renderer'

const state = vi.hoisted(() => ({
  subscribe: vi.fn(), toast: vi.fn(), fetchEntries: vi.fn(), fetchTags: vi.fn(),
}))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { subscribe: state.subscribe } }))
vi.mock('@/components/toaster', () => ({ default: state.toast }))
vi.mock('@/lib/api/query', () => ({ feedsTagsGQL: 'tags', initLazyQuery: () => ({ fetch: state.fetchTags }) }))
vi.mock('vue-router', () => ({ useRoute: () => ({ path: '/feeds', query: {} }) }))
vi.mock('@/plugins/router', () => ({ replacePath: vi.fn() }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ feedsSyncing: ref(true) }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/hooks/list', () => ({ useSelectable: () => ({ total: ref(0) }) }))
vi.mock('@/hooks/feed-entries', () => ({ useList: () => ({ page: ref(1), fetch: state.fetchEntries, noMore: ref(false) }) }))
vi.mock('@/hooks/search', () => ({ useSearch: () => ({ parseQ: vi.fn() }) }))
vi.mock('@/hooks/key-events', () => ({ useKeyEvents: () => ({ keyDown: vi.fn(), keyUp: vi.fn() }) }))

afterEach(() => {
  vi.clearAllMocks()
  vi.unstubAllGlobals()
})

it.each(['', 'Feed unavailable'])('refreshes after feed sync and shows only failures (%s)', async (error) => {
  vi.stubGlobal('document', { getElementsByClassName: () => [] })
  vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() })
  const FeedView = defineComponent({
    setup() {
      const { feedsSyncing } = useFeedEntriesData(vi.fn())
      return () => h('div', feedsSyncing.value ? 'syncing' : 'idle')
    },
  })
  const view = renderView({ render: () => h(KeepAlive, null, () => h(FeedView)) })
  try {
    const [field, complete] = state.subscribe.mock.calls[0]!
    expect(field).toBe('feedsFetched')
    expect(content(view.root)).toBe('syncing')
    state.fetchEntries.mockClear()
    state.fetchTags.mockClear()
    complete({ error })
    await flush()
    expect(content(view.root)).toBe('idle')
    expect(state.fetchEntries).toHaveBeenCalledOnce()
    expect(state.fetchTags).toHaveBeenCalledOnce()
    if (error) expect(state.toast).toHaveBeenCalledExactlyOnceWith(error, 'error')
    else expect(state.toast).not.toHaveBeenCalled()
  } finally {
    view.unmount()
  }
})
