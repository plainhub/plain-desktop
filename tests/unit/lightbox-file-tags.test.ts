import { expect, it, vi } from 'vitest'
import { defineComponent, h, ref } from 'vue'
import LightboxFileTags from '@/components/lightbox/LightboxFileTags.vue'
import LightboxFileInfoItem from '@/components/lightbox/LightboxFileInfoItem.vue'
import { DataType } from '@/lib/data'
import ItemTags from '@/components/ItemTags.vue'
import { clientTemplate, content, flush, renderView } from '../helpers/vue-node-renderer'

vi.mock('@/plugins/router', () => ({ replacePath: vi.fn() }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))

clientTemplate(LightboxFileTags, 'src/components/lightbox/LightboxFileTags.vue')
clientTemplate(LightboxFileInfoItem, 'src/components/lightbox/LightboxFileInfoItem.vue')
clientTemplate(ItemTags, 'src/components/ItemTags.vue')

it('renders the selected labels after the lightbox receives tag relations', async () => {
  const relations = ref<{ tagId: string; key: string }[]>([])
  const current = { type: DataType.IMAGE, name: 'image.jpg', path: '/fixture/image.jpg', src: '', size: 4, durationMs: 0, data: { id: 'image' } }
  const tagsMap = new Map([['IMAGE', [{ id: 'tag', name: 'Landscape', count: 1 }]]])
  const host = defineComponent({ setup: () => () => h(LightboxFileTags, { current, tagsMap, itemTags: relations.value }) })
  const view = renderView(host, {}, { LightboxFileInfoItem, ItemTags, TagRelationsDropdown: { render: () => null } })
  try {
    expect(content(view.root)).not.toContain('#Landscape')
    relations.value = [{ key: 'image', tagId: 'tag' }]
    await flush()
    expect(content(view.root)).toContain('#Landscape')
  } finally {
    view.unmount()
  }
})
