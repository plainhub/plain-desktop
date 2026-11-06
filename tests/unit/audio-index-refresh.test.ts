import { afterEach, expect, it, vi } from 'vitest'
import { createSSRApp, defineComponent, h, onServerPrefetch, reactive, ref, toRefs } from 'vue'
import { renderToString } from '@vue/server-renderer'
const state = vi.hoisted(() => ({ handle: undefined as any, pageOptions: undefined as any, fetch: vi.fn(), loading: undefined as any, store: undefined as any, page: undefined as any, beforeRender: undefined as any, items: undefined as any }))
vi.mock('@/stores/main', () => ({ useMainStore: () => state.store }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ audioPlaying: ref(false) }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: any) => toRefs(reactive(store)) }))
vi.mock('@/lib/api/query', () => ({ audiosGQL: 'audios', initLazyQuery: ({ handle }: any) => { state.handle = handle; return { loading: state.loading, fetch: state.fetch } } }))
vi.mock('@/lib/api/file', () => ({ getFileId: (_: unknown, path: string) => path }))
vi.mock('@/lib/feature', () => ({ hasMediaTrash: () => false, hasMediaDelete: () => false, hasMediaMove: () => false }))
vi.mock('@/lib/file', () => ({ getSortItems: () => [], isAudio: () => true }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/hooks/audio-playlist-store', () => ({ audioPlayback: ref({}) }))
vi.mock('@/hooks/media-page-actions', () => ({ useMediaPageActions: () => ({}) }))
vi.mock('@/hooks/media', () => ({ useMoveToFolder: () => ({}) }))
vi.mock('@/views/audios/hooks/useAudiosHooks', () => ({ useAudioPlayer: () => ({}), useAddToPlaylist: () => ({}) }))
vi.mock('@/hooks/media-page', () => ({ useMediaPage: (options: any) => {
  state.pageOptions = options
  state.items = options.items
  onServerPrefetch(() => state.beforeRender?.())
  return {
    page: state.page, limit: ref(50), q: ref(''), filter: reactive({}), total: ref(0), sorting: ref(false),
    selectedIds: ref([]), shiftEffectingIds: ref([]), tags: ref([]), bucketsMap: ref({}), app: ref({ access: [] }),
    restoreLoading: () => false, trashLoading: () => false, getQuery: () => '', urlTokenKey: ref(null),
  }
} }))
vi.mock('@/components/media/MediaToolbar.vue', () => ({ default: defineComponent({ render: () => h('header') }) }))
vi.mock('@/components/media/MediaPageActions.vue', () => ({ default: defineComponent({ render: () => null }) }))
vi.mock('@/components/NoDataPlaceholder.vue', () => ({ default: defineComponent({ props: ['loading'], setup: (props) => () => h('div', { class: 'empty', 'data-loading': String(props.loading) }) }) }))
vi.mock('@/views/audios/AudioListItem.vue', () => ({ default: defineComponent({ props: ['item'], setup: (props) => () => h('div', { class: 'audio', 'data-id': props.item.id }) }) }))
vi.mock('@/views/audios/AudioSkeletonItem.vue', () => ({ default: defineComponent({ render: () => h('div', { class: 'skeleton' }) }) }))
import AudiosView from '@/views/audios/AudiosView.vue'
async function render(beforeRender?: () => void) {
  state.store = reactive({ audioSortBy: 'DATE_DESC', audiosScrollPaging: true })
  state.page = ref(1)
  state.loading = ref(true)
  state.fetch.mockReset()
  state.beforeRender = beforeRender
  const app = createSSRApp(AudiosView)
  app.config.globalProperties.$t = (key: string) => key
  for (const name of ['v-icon-button', 'v-pagination', 'BulkTagDropdown']) app.component(name, { render: () => null })
  app.directive('tooltip', {})
  return renderToString(app)
}
afterEach(() => vi.unstubAllGlobals())
it('keeps the empty state visible during index refresh instead of flashing skeletons', async () => {
  expect((await render()).match(/class="skeleton"/g)).toHaveLength(20)
  const html = await render(() => {
    state.handle({ items: [], total: 0 }, '', { variables: { offset: 0, limit: 50 } })
    state.loading.value = true
    state.pageOptions.doFetch({ force: true, latest: true })
  })
  expect(html).not.toContain('class="skeleton"')
  expect(html).toContain('class="empty" data-loading="false"')
})
it('replaces the loaded audio window on index refresh and appends only actual next pages', async () => {
  const audio = (id: string) => ({ id, path: `/${id}.ogg` })
  const html = await render(() => {
    state.handle({ items: [audio('first')], total: 100 }, '', { variables: { offset: 0, limit: 50 } })
    state.page.value = 2
    state.handle({ items: [audio('second')], total: 100 }, '', { variables: { offset: 50, limit: 50 } })
    expect(state.items.value.map((item: any) => item.id)).toEqual(['first', 'second'])
    state.pageOptions.doFetch({ force: true, latest: true })
    expect(state.fetch).toHaveBeenLastCalledWith({ offset: 0, limit: 100, query: '', sortBy: 'DATE_DESC' }, { force: true, latest: true })
    expect(state.items.value.map((item: any) => item.id)).toEqual(['first', 'second'])
    state.handle({ items: [audio('first'), audio('new')], total: 2 }, '', { variables: { offset: 0, limit: 100 } })
    expect(state.items.value.map((item: any) => item.id)).toEqual(['first', 'new'])
  })
  expect(html.match(/class="audio"/g)).toHaveLength(2)
  expect(html).not.toContain('data-id="second"')
  expect(html).not.toContain('class="skeleton"')
})
