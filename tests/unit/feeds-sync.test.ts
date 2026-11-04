import { ref } from 'vue'
import { expect, it, vi } from 'vitest'
import { useFeedEntriesActions } from '@/views/feeds/hooks/useFeedEntriesActions'

const mutate = vi.hoisted(() => vi.fn())
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ feedsSyncing: ref(false) }) }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('@/hooks/feeds', () => ({ useFeeds: () => ({ viewFeed: vi.fn() }) }))
vi.mock('@/hooks/list', () => ({ useDelete: () => ({}) }))
vi.mock('@/plugins/router', () => ({ default: {}, replacePath: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/lib/api/mutation', () => ({
  deleteFeedEntriesGQL: 'delete', saveFeedEntriesToNotesGQL: 'notes', syncFeedsGQL: 'sync', deleteFeedEntryGQL: 'delete-one',
  initMutation: ({ document }: { document: string }) => ({
    mutate: document === 'sync' ? mutate : vi.fn(), loading: ref(false), onDone: vi.fn(),
  }),
}))

it('requests all subscriptions with the optional feed ID unset', () => {
  const actions = useFeedEntriesActions({
    selectedIds: ref([]), realAllChecked: ref(false), q: ref('feed_id:selected'), total: ref(0),
    tags: ref([]), items: ref([]), clearSelection: vi.fn(), fetch: vi.fn(),
  })
  actions.syncFeeds()
  expect(mutate).toHaveBeenCalledExactlyOnceWith({ id: null })
})
