import { expect, it, vi } from 'vitest'
import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { createPinia, defineStore } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import AppRail from '@/views/app-rail/AppRail.vue'
import { getAvailableHomeFeatures } from '@/views/home/features'

vi.mock('@/stores/main', () => ({
  useMainStore: defineStore('image-editor-navigation-main', {
    state: () => ({ railFeatures: ['IMAGE_EDITOR', 'SMS'], lastRoutes: {} }),
  }),
}))
vi.mock('@/stores/temp', () => ({
  useTempStore: defineStore('image-editor-navigation-temp', {
    state: () => ({ app: { capabilities: [], debug: false } }),
  }),
}))
vi.mock('@/views/app-rail/RailSettingsPopup.vue', () => ({ default: () => null }))

it('renders the configured Image Editor rail link in a non-debug session without exposing unsupported features', async () => {
  const app = createSSRApp(AppRail)
  const router = createRouter({
    history: createMemoryHistory(),
    routes: ['/', '/image-editor', '/messages'].map(path => ({ path, component: { render: () => null } })),
  })
  app.use(createPinia())
  app.use(router)
  app.config.globalProperties.$t = (key: string) => key
  app.directive('tooltip', {})
  await router.push('/')
  const html = await renderToString(app)
  expect(html).toMatch(/<a[^>]*href="\/image-editor"[^>]*aria-label="page_title.image_editor"/)
  expect(html).not.toContain('page_title.messages')
  expect(getAvailableHomeFeatures([]).map(feature => feature.id)).toContain('IMAGE_EDITOR')
})
