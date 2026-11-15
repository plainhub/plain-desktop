import { expect, it, vi } from 'vitest'
import { createSSRApp, h, ref } from 'vue'
import { renderToString } from 'vue/server-renderer'

vi.mock('pinia', () => ({ storeToRefs: (value: unknown) => value }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({ capabilities: ['MEDIA_SCAN'] }), counter: ref({ total: 0, free: 0 }) }) }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => true }))
vi.mock('@/views/home/home', () => ({ useHomeData: () => ({ mounts: ref([]) }) }))
vi.mock('@/views/home/useHomeFeatureCards', () => ({ useHomeFeatureCards: () => ({ homeFeatureCards: ref([{ id: 'files', sectionType: 'feature', titleKey: 'files' }, { id: 'images', sectionType: 'feature', titleKey: 'images' }]) }) }))
vi.mock('@/views/home/FeatureCard.vue', () => ({ default: { props: ['title'], setup: (props: any) => () => h('article', props.title) } }))
vi.mock('@/views/home/file-index/FileIndexCard.vue', () => ({ default: { render: () => h('section', 'file-index') } }))
vi.mock('@/views/home/remote-access/RemoteAccessCard.vue', () => ({ default: { render: () => h('section', 'http-server') } }))
import HomeView from '@/views/home/HomeView.vue'

it('renders feature cards before file indexing and HTTP server', async () => {
  const app = createSSRApp(HomeView)
  app.config.globalProperties.$t = (key: string) => key
  const html = await renderToString(app)
  for (const label of ['files', 'images', 'file-index', 'http-server']) expect(html).toContain(`>${label}<`)
  expect(html.indexOf('>files<')).toBeLessThan(html.indexOf('>images<'))
  expect(html.indexOf('>images<')).toBeLessThan(html.indexOf('>file-index<'))
  expect(html.indexOf('>file-index<')).toBeLessThan(html.indexOf('>http-server<'))
})
