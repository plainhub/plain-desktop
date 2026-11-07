import { computed, createApp, h, ref } from 'vue'
import { createPinia } from 'pinia'
import { createI18n } from 'vue-i18n'
import { createMemoryHistory, createRouter, RouterView } from 'vue-router'
import GlobalSearchSidebar from '@/views/search/GlobalSearchSidebar.vue'
import GlobalSearchView from '@/views/search/GlobalSearchView.vue'
import SearchDeviceDropdown from '@/views/search/SearchDeviceDropdown.vue'
import Lightbox from '@/components/lightbox/lightbox.vue'
import { globalSearchKey, type GlobalSearch, type SearchGroup } from '@/hooks/global-search'
import { searchDomains, type SearchDevice, type SearchHit } from '@/lib/api/global-search'
import labels from '@/locales/zh-CN'
import tooltip from '@/plugins/tooltip'
import '@/styles/main.scss'

const deviceCount = Number(new URLSearchParams(window.location.search).get('devices')) || 3
const fixtureDevices: SearchDevice[] = [
  { id: 'current', name: 'Studio Mac', target: 'current' },
  { id: 'archive', name: 'Home NAS', target: 'local' },
  { id: 'phone', name: 'Pixel', target: 'local' },
]
const devices = ref(fixtureDevices.slice(0, deviceCount))
const data = { id: 'one', createdAt: '2026-01-01T00:00:00Z', updatedAt: '2026-01-01T00:00:00Z', tags: [] }
const file = { ...data, size: 1024, bucketId: 'travel' }
const images: SearchHit[] = [
  { domain: 'images', item: { ...file, title: '旅行照片.jpg', path: '/photos/travel.jpg' } },
  { domain: 'images', item: { ...file, title: '旅行地图.png', path: '/archive/map.png' } },
]
const video: SearchHit = { domain: 'videos', item: { ...file, title: '旅行视频.mp4', path: '/archive/travel.mp4', durationMs: 42000 } }
const document: SearchHit = { domain: 'docs', item: { ...file, title: '旅行计划.md', path: '/notes/plan.md', extension: 'md' } }
const note: SearchHit = { domain: 'notes', item: { ...data, title: '旅行笔记' } }
const feed: SearchHit = { domain: 'feeds', item: { ...data, title: '旅行订阅', author: '', feedId: 'travel', url: '', image: '', rawId: '', publishedAt: '' } }
const collapsed = ref(false)
const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: '/', component: { render: () => h('p', '首页') } },
    { path: '/search', name: 'search', components: { default: GlobalSearchView, LeftSidebar: GlobalSearchSidebar } },
    { path: '/notes/:id', component: { render: () => h('p', note.item.title) } },
    { path: '/feeds/:feedId/entries/:id', component: { render: () => h('p', feed.item.title) } },
  ],
})
const selected = ref(devices.value.map(({ id }) => id))
const domain = computed(() => searchDomains.find((kind) => kind === router.currentRoute.value.query.type) ?? 'all')
const targets = computed(() => devices.value.filter(({ id }) => selected.value.includes(id)))
const groups = computed<SearchGroup[]>(() => targets.value.map((device) => {
  if (device.id === 'phone') return { device, status: 'offline', pages: [] }
  const hits = device.id === 'current' ? [images[0], document, note, feed] : [images[1], video]
  return {
    device, status: 'ready',
    pages: hits.filter((hit) => domain.value === 'all' || hit.domain === domain.value)
      .map((hit) => ({ domain: hit.domain, hits: [hit], total: 1, loading: false, failed: false })),
    client: { name: device.name, url: () => '/icons/76.png?fixture=1', thumbnailUrl: () => '/icons/76.png?fixture=1', fileId: () => '', canSearch: () => true, page: async () => ({ hits: [], total: 0 }) },
  }
}))
const search: GlobalSearch = {
  devices, targets, domain, groups,
  request: ref({ text: '旅行', foreground: false }),
  total: computed(() => groups.value.reduce((sum, group) => sum + group.pages.length, 0)),
  select: (id, checked) => { selected.value = checked ? [...selected.value, id] : selected.value.filter((value) => value !== id) },
  selectAll: (checked) => { selected.value = checked ? devices.value.map(({ id }) => id) : [] },
  submit: () => {}, loadMore: () => {}, retry: () => {}, reset: () => {},
}

const app = createApp({ setup: () => () => h('div', { class: 'page-content' }, [
  h(RouterView, { name: 'LeftSidebar' }),
  h('main', { class: 'main' }, [
    h(SearchDeviceDropdown, { search }),
    h(RouterView),
  ]),
  h(Lightbox),
]) })
app.provide('isPhone', ref(false)).provide('isTablet', ref(false))
app.provide(globalSearchKey, { search, sidebarCollapsed: collapsed })
app.use(createPinia()).use(router).use(tooltip).use(createI18n({ legacy: false, locale: 'zh-CN', messages: { 'zh-CN': labels } }))
await router.push({ name: 'search' })
app.mount('#app')
