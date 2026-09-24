import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, h } from 'vue'
import AppRail from '@/views/app-rail/AppRail.vue'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { Capability } from '@/lib/data'

const RailSettingsPopupStub = defineComponent({
  name: 'RailSettingsPopup',
  setup: () => () => h('div', { class: 'settings-stub' }),
})

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/files/recent', component: { template: '<div />' } },
    ],
  })
}

function mountRail() {
  const router = makeRouter()
  const pinia = createPinia()
  setActivePinia(pinia)
  const tempStore = useTempStore()
  tempStore.app = { ...tempStore.app, capabilities: Object.values(Capability) }
  const wrapper = mount(AppRail, {
    global: {
      plugins: [router, pinia],
      mocks: { $t: (key: string) => key },
      directives: { tooltip: () => {} },
      stubs: { RailSettingsPopup: RailSettingsPopupStub },
    },
  })
  const store = useMainStore(pinia)
  store.railFeatures = ['files', 'notes']
  return { wrapper, router, store }
}

describe('AppRail', () => {
  it('renders home as the first rail item shared by desktop and mobile', async () => {
    const { wrapper, router } = mountRail()
    await router.push('/')
    await router.isReady()
    await wrapper.vm.$nextTick()

    const items = wrapper.findAll('.rail-item')
    expect(items.length).toBeGreaterThanOrEqual(3)
    const home = items[0]
    expect(home.attributes('href')).toBe('/')
    expect(home.attributes('aria-label')).toBe('page_title.home')
    expect((home.element as HTMLElement).children.length).toBe(2)
    expect(home.text()).toContain('page_title.home')
  })

  it('marks home active only on the root route', async () => {
    const { wrapper, router } = mountRail()
    await router.push('/')
    await router.isReady()
    await wrapper.vm.$nextTick()
    expect(wrapper.findAll('.rail-item')[0].classes()).toContain('active')

    await router.push('/files/recent')
    await wrapper.vm.$nextTick()
    expect(wrapper.findAll('.rail-item')[0].classes()).not.toContain('active')
    expect(wrapper.findAll('.rail-item')[1].classes()).toContain('active')
  })

  it('no longer renders the brand logo block', async () => {
    const { wrapper } = mountRail()
    expect(wrapper.find('.rail-brand').exists()).toBe(false)
    expect(wrapper.find('.brand-logo').exists()).toBe(false)
    expect(wrapper.find('.rail-item.rail-home').exists()).toBe(false)
  })

  it('keeps the settings popup outside the scrollable items row', () => {
    const { wrapper } = mountRail()
    expect(wrapper.find('.rail-items .settings-stub').exists()).toBe(false)
    expect(wrapper.find('.settings-stub').exists()).toBe(true)
  })
})
