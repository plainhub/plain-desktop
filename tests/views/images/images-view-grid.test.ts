import { mount } from '@vue/test-utils'
import { defineComponent, h, nextTick, KeepAlive, ref } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'
import { createPinia, setActivePinia } from 'pinia'
import { describe, expect, it, vi } from 'vitest'

const lazyQueryOpts: Record<string, any> = {}

vi.mock('@/lib/api/query', async (importOriginal) => {
  const actual = await importOriginal<any>()
  return {
    ...actual,
    initLazyQuery: (opts: any) => {
      lazyQueryOpts.images = opts
      return { loading: ref(false), fetch: vi.fn() }
    },
  }
})

import emitter from '@/plugins/eventbus'
import ImagesView from '@/views/images/ImagesView.vue'
import { useTempStore } from '@/stores/temp'
import VIconButton from '@/components/base/VIconButton.vue'
import VCheckCircle from '@/components/base/VCheckCircle.vue'
import i18n from '@/plugins/i18n'

const makeRouter = async () => {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/images', component: { template: '<div />' } }, { path: '/', component: { template: '<div />' } }],
  })
  router.push('/images')
  await router.isReady()
  return router
}

const Host = defineComponent({
  setup: () => () => h(KeepAlive, () => h(ImagesView)),
})

const mountView = async () => {
  const router = await makeRouter()
  const pinia = createPinia()
  setActivePinia(pinia)
  const wrapper = mount(Host, {
    global: {
      plugins: [router, pinia, i18n],
      components: { VIconButton, VCheckCircle },
      directives: { tooltip: {} },
      stubs: { ItemTags: true, ImageSearchButton: true, VPagination: true },
    },
  })
  return wrapper
}

const feed = (n: number) => {
  lazyQueryOpts.images.handle({
    images: Array.from({ length: n }, (_, i) => ({
      id: `id-${i}`, path: `/dcim/p${i}.jpg`, size: 1024, updatedAt: '2026-09-15T00:00:00Z', tags: [], bucketId: 'b',
    })),
    imageCount: n,
  })
}

const tiles = (wrapper: any) => wrapper.findAll('section.media-item')
const selectedTiles = (wrapper: any) => tiles(wrapper).filter((t: any) => t.classes().includes('selected')).length
const checkedCircles = (wrapper: any) => wrapper.findAll('.check-circle.checked').length

describe('ImagesView grid selection wiring', () => {
  it('keeps every tile in sync with the selection (v-memo safety)', { timeout: 60_000 }, async () => {
    const wrapper = await mountView()
    feed(3)
    await nextTick()
    await nextTick()
    expect(tiles(wrapper).length).toBe(3)
    expect(selectedTiles(wrapper)).toBe(0)
    expect(checkedCircles(wrapper)).toBe(0)

    await tiles(wrapper)[0].find('button.btn-checkbox').trigger('click')
    await nextTick()
    expect(selectedTiles(wrapper)).toBe(1)
    expect(checkedCircles(wrapper)).toBe(1)

    await tiles(wrapper)[1].find('button.btn-checkbox').trigger('click')
    await tiles(wrapper)[2].find('button.btn-checkbox').trigger('click')
    await nextTick()
    expect(selectedTiles(wrapper)).toBe(3)
    expect(checkedCircles(wrapper)).toBe(3)

    await tiles(wrapper)[1].find('button.btn-checkbox').trigger('click')
    await nextTick()
    expect(selectedTiles(wrapper)).toBe(2)
    expect(checkedCircles(wrapper)).toBe(2)
    expect(tiles(wrapper)[1].classes()).not.toContain('selected')
    expect(tiles(wrapper)[0].classes()).toContain('selected')
    expect(tiles(wrapper)[2].classes()).toContain('selected')
  })

  it('shift-click selects the range between anchor and target', { timeout: 60_000 }, async () => {
    const wrapper = await mountView()
    feed(4)
    await nextTick()
    await nextTick()

    await tiles(wrapper)[0].find('button.btn-checkbox').trigger('click')
    await tiles(wrapper)[3].trigger('click', { shiftKey: true })
    await nextTick()

    expect(selectedTiles(wrapper)).toBe(4)
    expect(checkedCircles(wrapper)).toBe(4)
  })

  it('opens the lightbox at the fresh index after a realtime item removal', { timeout: 60_000 }, async () => {
    const wrapper = await mountView()
    const tempStore = useTempStore()
    feed(4)
    await nextTick()
    await nextTick()

    emitter.emit('media_items_actioned', { type: 'IMAGE', action: 'trash', id: 'id-1' })
    await nextTick()
    await nextTick()
    expect(tiles(wrapper).length).toBe(3)

    await tiles(wrapper)[1].trigger('click')
    await nextTick()

    expect(tempStore.lightbox?.index).toBe(1)
    expect(tempStore.lightbox?.sources.length).toBe(3)
  })
})
