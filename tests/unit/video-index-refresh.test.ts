import { expect, it, vi } from 'vitest'
import { createSSRApp, h, onServerPrefetch, reactive, ref, toRefs } from 'vue'
import { renderToString } from '@vue/server-renderer'

const state = vi.hoisted(() => ({
  fetch: vi.fn(), handle: undefined as any, beforeRender: undefined as any,
}))
vi.mock('pinia', () => ({ storeToRefs: (store: any) => toRefs(store) }))
vi.mock('@/stores/main', () => ({ useMainStore: () => store }))
vi.mock('@/lib/api/query', () => ({
  videosGQL: 'videos', imagesGQL: 'images',
  initLazyQuery: ({ handle }: any) => {
    state.handle = handle
    return { loading: ref(false), fetch: state.fetch }
  },
}))
vi.mock('@/lib/api/file', () => ({ getFileId: vi.fn(), getFileUrl: vi.fn(), getFileExtension: vi.fn(), getFileName: vi.fn() }))
vi.mock('@/lib/file', () => ({ getSortItems: () => [], getVideoGroupByItems: () => [], isVideo: () => true, getImageSortItems: () => [], getImageGroupByItems: () => [], isImage: () => true, getGroupDateKey: () => '2026-01-01', formatGroupDateLabel: (day: string) => day }))
vi.mock('@/lib/feature', () => ({ hasFeature: () => true, hasMediaMove: () => false }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/hooks/media', () => ({ useMoveToFolder: () => ({}) }))
vi.mock('@/hooks/media-page-actions', () => ({ useMediaPageActions: () => ({}) }))
vi.mock('@/hooks/open-media', () => ({ useOpenMedia: () => ({ open: vi.fn() }) }))
vi.mock('@/hooks/media-page', () => ({
  useMediaPage: (options: any) => {
    const page = ref(1)
    onServerPrefetch(() => state.beforeRender(page, options.doFetch))
    return {
      mainStore: store, page, limit: ref(2), q: ref(''), filter: reactive({}), total: ref(0), sorting: ref(false),
      selectedIds: ref([]), selectedIdSet: ref(new Set()), shiftEffectingIdSet: ref(new Set()), shiftEffectingIds: ref([]), tags: ref([]), bucketsMap: ref({}),
      app: ref({ access: [] }), urlTokenKey: ref(undefined),
      restoreLoading: () => false, trashLoading: () => false, getQuery: () => '',
    }
  },
}))
vi.mock('@/components/media/MediaToolbar.vue', () => ({ default: { render: () => h('header') } }))
vi.mock('@/components/media/MediaPageActions.vue', () => ({ default: { render: () => null } }))
vi.mock('@/components/media/MediaGridItem.vue', () => ({ default: {
  props: ['item'], setup: (props: { item: { id: string; path: string } }) =>
    () => h('div', { class: 'image-row', 'data-id': props.item.id }, props.item.path),
} }))
vi.mock('@/components/ai/ImageSearchButton.vue', () => ({ default: { render: () => null } }))
vi.mock('@/components/NoDataPlaceholder.vue', () => ({ default: { render: () => null } }))
import VideosView from '@/views/videos/VideosView.vue'
import ImagesView from '@/views/images/ImagesView.vue'

const store = reactive({ videoSortBy: 'NAME_ASC', videosCardView: true, videosGroupBy: '', videosScrollPaging: true, imageSortBy: 'NAME_ASC', imagesCardView: false, imagesGroupBy: 'TAKEN_AT', imagesScrollPaging: false })

it('replaces the loaded video window when an index update arrives on page two', async () => {
  const video = (id: string, name: string) => ({ id, path: '/videos/' + name })
  const old = video('1', 'Agenda.mp4')
  const renamed = video('1', 'Schedule.mp4')
  const second = video('2', 'Demo.mp4')
  const third = video('3', 'Intro.mp4')
  const fourth = video('4', 'Readme.mp4')
  const respond = (videos: ReturnType<typeof video>[], offset: number, limit: number) =>
    state.handle({ videos, videoCount: 4 }, '', { variables: { offset, limit } })

  state.beforeRender = async (page: ReturnType<typeof ref<number>>, doFetch: (options: { force: boolean; latest: boolean }) => void) => {
    await respond([old, second], 0, 2)
    page.value = 2
    await respond([third, fourth], 2, 2)
    doFetch({ force: true, latest: true })
    expect(state.fetch).toHaveBeenLastCalledWith(
      { offset: 0, limit: 4, query: '', sortBy: 'NAME_ASC' }, { force: true, latest: true },
    )
    await respond([second, third, fourth, renamed], 0, 4)
  }

  const app = createSSRApp(VideosView)
  app.config.globalProperties.$t = (key: string) => key
  app.directive('tooltip', {})
  for (const name of ['BulkTagDropdown', 'v-icon-button', 'v-checkbox', 'image-video-list-skeleton', 'v-pagination']) {
    app.component(name, { render: () => null })
  }
  app.component('VideoListItem', {
    props: { item: { type: Object, required: true } },
    setup: ({ item }) => () => h('div', { class: 'video-row', 'data-id': item.id }, item.path),
  })
  const html = await renderToString(app)
  expect(html.match(/class="video-row"/g)).toHaveLength(4)
  expect(html.match(/data-id="1"/g)).toHaveLength(1)
  expect(html).not.toContain('Agenda.mp4')
  expect(html).toContain('/videos/Schedule.mp4')
})

it('refreshes the complete loaded window in grouped ImagesView and replaces stale rows', async () => {
  const image = (id: string, name: string) => ({ id, path: '/images/' + name, updatedAt: '' })
  const first = image('1', 'Cover.png')
  const second = image('2', 'Old.png')
  const third = image('3', 'Diagram.png')
  const fourth = image('4', 'Poster.png')
  const respond = (images: ReturnType<typeof image>[], offset: number, limit: number) =>
    state.handle({ images, imageCount: images.length }, '', { variables: { offset, limit } })

  state.beforeRender = async (page: ReturnType<typeof ref<number>>, doFetch: (options: { force: boolean; latest: boolean }) => void) => {
    await respond([first, second], 0, 2)
    page.value = 2
    await respond([third, fourth], 2, 2)
    doFetch({ force: true, latest: true })
    expect(state.fetch).toHaveBeenLastCalledWith(
      { offset: 0, limit: 4, query: '', sortBy: 'TAKEN_AT_DESC' }, { force: true, latest: true },
    )
    await respond([fourth, third, first, image('5', 'New.png')], 0, 4)
  }

  const app = createSSRApp(ImagesView)
  app.config.globalProperties.$t = (key: string) => key
  app.directive('tooltip', {})
  const html = await renderToString(app)
  expect(html.match(/class="image-row"/g)).toHaveLength(4)
  expect(html).toContain('/images/Poster.png')
  expect(html).toContain('/images/New.png')
  expect(html).not.toContain('/images/Old.png')
})
