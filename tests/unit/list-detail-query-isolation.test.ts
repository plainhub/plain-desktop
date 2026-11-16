import { expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { encodeBase64 } from '@/lib/strutil'
import type { INote, IFeedEntry } from '@/lib/interfaces'

const state = vi.hoisted(() => ({ navigate: vi.fn() }))
vi.mock('@/plugins/router', () => ({ default: { currentRoute: { value: { query: { q: 'foreign-file-filter' } } } }, replacePath: state.navigate }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ feedsSyncing: ref(false) }) }))
vi.mock('@/hooks/list', () => ({ useDelete: () => ({}) }))
vi.mock('@/hooks/notes', () => ({ useNotesTrash: () => ({}), useNotesRestore: () => ({}) }))
vi.mock('@/hooks/feeds', () => ({ useFeeds: () => ({}) }))
vi.mock('@/lib/api/query', () => ({ exportNotesGQL: 'exportNotes' }))
vi.mock('@/lib/api/file', () => ({ download: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: vi.fn() }))
vi.mock('@/lib/api/mutation', () => ({
  deleteNotesGQL: 'deleteNotes', deleteNoteGQL: 'deleteNote', deleteFeedEntriesGQL: 'deleteFeedEntries',
  saveFeedEntriesToNotesGQL: 'saveFeedEntriesToNotes', syncFeedsGQL: 'syncFeeds', deleteFeedEntryGQL: 'deleteFeedEntry',
  initMutation: () => ({ mutate: vi.fn(), onDone: vi.fn(), loading: ref(false) }),
}))
import { useNotesActions } from '@/views/notes/hooks/useNotesActions'
import { useFeedEntriesActions } from '@/views/feeds/hooks/useFeedEntriesActions'

it.each(['', 'tag_id:synthetic text:Original'])('builds detail links from the owning list query: %s', query => {
  state.navigate.mockClear()
  const options = { selectedIds: ref<string[]>([]), realAllChecked: ref(false), q: ref(query), total: ref(0), clearSelection: vi.fn(), fetch: vi.fn() }
  const notes = useNotesActions({ ...options, items: ref<INote[]>([]) })
  const feeds = useFeedEntriesActions({ ...options, items: ref<IFeedEntry[]>([]), tags: ref([]) })
  const note = { id: 'synthetic-note' } as INote
  const entry = { id: 'synthetic-entry', feedId: 'synthetic-feed' } as IFeedEntry
  const suffix = query ? `?q=${encodeBase64(query)}` : ''
  expect(notes.viewUrl(note)).toBe(`/notes/synthetic-note${suffix}`)
  expect(feeds.viewUrl(entry)).toBe(`/feeds/synthetic-feed/entries/synthetic-entry${suffix}`)
  notes.view(note)
  feeds.view(entry)
  expect(state.navigate.mock.calls.map(([, path]) => path)).toEqual([
    `/notes/synthetic-note${suffix}`, `/feeds/synthetic-feed/entries/synthetic-entry${suffix}`,
  ])
})
