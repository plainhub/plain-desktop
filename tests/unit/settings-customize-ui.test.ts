import { expect, it, vi } from 'vitest'
import { createRenderer, nextTick, reactive, ssrContextKey } from 'vue'

const fixture = vi.hoisted(() => ({ store: null as any }))
vi.mock('@/stores/main', () => ({ useMainStore: () => fixture.store }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: {} }) }))
vi.mock('pinia', () => ({ storeToRefs: () => ({ app: { value: {} } }) }))
vi.mock('@/views/app-rail/features', () => ({ getAvailableFeatures: () => [{ id: 'FILES' }, { id: 'IMAGES' }] }))
vi.mock('@/views/home/features', () => ({
  getAvailableHomeFeatures: () => [{ id: 'FILES' }, { id: 'NOTES' }],
  normalizeHomeFeatures: (ids: string[], available: string[]) => ids.filter(id => available.includes(id)),
}))
vi.mock('@/views/app-rail/FeatureSortToggleList.vue', () => ({ default: {} }))
import SettingsCustomizeUIView from '@/views/settings/SettingsCustomizeUIView.vue'

it('updates the selected customization group and follows later store updates on the cached page', async () => {
  fixture.store = reactive({ railFeatures: ['FILES'], homeFeatures: ['FILES'] })
  const renderer = createRenderer<any, any>({
    insert: vi.fn(), remove: vi.fn(), createElement: tag => ({ tag }),
    createText: () => ({}), createComment: () => ({}), setText: vi.fn(), setElementText: vi.fn(),
    parentNode: () => null, nextSibling: () => null,
    patchProp: vi.fn(),
  })
  const app = renderer.createApp({ ...SettingsCustomizeUIView, render: () => null })
  app.config.globalProperties.$t = (key: string) => key
  app.provide(ssrContextKey, { modules: new Set() })
  const vm = app.mount({})
  const state = vm.$.setupState as any
  try {
    expect(state.activeEnabledIds).toEqual(['FILES'])
    state.activeEnabledIds = ['IMAGES', 'FILES']
    await nextTick()
    expect(fixture.store.railFeatures).toEqual(['IMAGES', 'FILES'])
    expect(fixture.store.homeFeatures).toEqual(['FILES'])
    state.activeTab = 'home'
    await nextTick()
    state.activeEnabledIds = ['NOTES']
    await nextTick()
    expect(fixture.store.homeFeatures).toEqual(['NOTES'])
    fixture.store.homeFeatures = ['FILES', 'NOTES']
    await nextTick()
    expect(state.activeEnabledIds).toEqual(['FILES', 'NOTES'])
    state.activeTab = 'sidebar'
    await nextTick()
    expect(state.activeEnabledIds).toEqual(['IMAGES', 'FILES'])
  } finally { app.unmount() }
})
