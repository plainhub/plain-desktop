import { expect, it, vi } from 'vitest'
import { createSSRApp, h, onServerPrefetch, reactive, ref, toRefs } from 'vue'
import { renderToString } from '@vue/server-renderer'

const state = vi.hoisted(() => ({
  fetch: vi.fn(), page: undefined as any, handle: undefined as any, options: undefined as any,
  store: undefined as any, beforeRender: undefined as any,
}))
vi.mock('pinia', () => ({ storeToRefs: (store: any) => toRefs(store) }))
vi.mock('@/stores/main', () => ({ useMainStore: () => state.store }))
vi.mock('@/lib/api/query', () => ({ docsGQL: 'docs', initLazyQuery: ({ handle }: any) => {
  state.handle = handle
  return { loading: ref(false), fetch: state.fetch }
} }))
vi.mock('@/lib/api/mutation', () => ({ deleteMediaItemsGQL: 'delete', initMutation: () => ({ onDone: vi.fn() }) }))
vi.mock('@/lib/file', () => ({ getSortItems: () => [], isDoc: () => true }))
vi.mock('@/lib/feature', () => ({ hasMediaTrash: () => false, hasMediaDelete: () => false, hasMediaMove: () => false }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/hooks/media', () => ({ useMoveToFolder: () => ({}) }))
vi.mock('@/hooks/media-page-actions', () => ({ useMediaPageActions: () => ({}) }))
vi.mock('@/views/docs/hooks/useDocsActions', () => ({ useDocsActions: () => ({}) }))
vi.mock('@/hooks/media-page', () => ({ useMediaPage: (options: any) => {
  state.options = options
  onServerPrefetch(() => state.beforeRender())
  return {
    page: state.page, limit: ref(2), q: ref(''), filter: reactive({}), total: ref(0), sorting: ref(false),
    selectedIds: ref([]), shiftEffectingIds: ref([]), tags: ref([]), bucketsMap: ref({}), app: ref({ access: [] }),
    restoreLoading: () => false, trashLoading: () => false, getQuery: () => '',
    refreshItems: () => options.doFetch({ force: true, latest: true }),
  }
} }))
vi.mock('@/components/media/MediaToolbar.vue', () => ({ default: { render: () => h('header') } }))
vi.mock('@/components/media/MediaPageActions.vue', () => ({ default: { render: () => null } }))
vi.mock('@/components/NoDataPlaceholder.vue', () => ({ default: { render: () => null } }))
vi.mock('@/views/docs/DocListItem.vue', () => ({ default: { props: { item: { type: Object, required: true } }, setup: (props: { item: { path: string; title: string } }) =>
  () => h('div', { class: 'doc-row', 'data-path': props.item.path }, props.item.title),
} }))
vi.mock('@/views/docs/DocSkeletonItem.vue', () => ({ default: { render: () => null } }))

import DocsView from '@/views/docs/DocsView.vue'

it('replaces all loaded rows after a successful rename, without losing the current scroll window', async () => {
  state.store = reactive({ docSortBy: 'NAME_ASC', docsScrollPaging: true })
  state.page = ref(1)
  const doc = (id: string, title: string) => ({ id, title, path: '/docs/' + title })
  const second = doc('2', 'Manual.md')
  const third = doc('3', 'Notes.md')
  const fourth = doc('4', 'Readme.md')
  state.beforeRender = () => {
    state.handle({ items: [doc('1', 'Agenda.md'), second], total: 4 }, '', { variables: { offset: 0, limit: 2 } })
    state.page.value = 2
    state.handle({ items: [third, fourth], total: 4 }, '', { variables: { offset: 2, limit: 2 } })
    state.options.doFetch({ force: true, latest: true })
    expect(state.fetch).toHaveBeenLastCalledWith(
      { offset: 0, limit: 4, query: '', sortBy: 'NAME_ASC' }, { force: true, latest: true },
    )
    state.handle({ items: [second, third, fourth, doc('1', 'Schedule.md')], total: 4 }, '', { variables: { offset: 0, limit: 4 } })
  }
  const app = createSSRApp(DocsView)
  app.config.globalProperties.$t = (key: string) => key
  for (const name of ['v-icon-button', 'v-pagination', 'BulkTagDropdown']) app.component(name, { render: () => null })
  app.directive('tooltip', {})
  const html = await renderToString(app)
  expect(html.match(/class="doc-row"/g)).toHaveLength(4)
  expect(html).not.toContain('Agenda.md')
  expect(html).toMatch(/data-path="\/docs\/Schedule\.md"[^>]*>Schedule\.md/)
})
