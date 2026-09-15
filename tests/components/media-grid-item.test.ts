import { mount } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { describe, expect, it } from 'vitest'
import { DataType } from '@/lib/data'
import MediaGridItem from '@/components/media/MediaGridItem.vue'

const VIconButtonStub = defineComponent({
  setup: (_, { slots }: any) => () => h('button', { class: 'icon-button-stub' }, slots.default?.()),
})
const VCheckCircleStub = defineComponent({
  props: { checked: { type: Boolean, default: false } },
  setup: (props: any) => () => h('span', { class: props.checked ? 'check-circle checked' : 'check-circle' }),
})

const mountItem = (props: Record<string, unknown>) =>
  mount(MediaGridItem, {
    props: {
      item: { id: 'a', tags: [] },
      checked: false,
      selected: false,
      shiftSelected: false,
      shouldSelect: false,
      dataType: DataType.IMAGE,
      ...props,
    },
    global: {
      components: { VIconButton: VIconButtonStub, VCheckCircle: VCheckCircleStub },
      directives: { tooltip: {} },
      mocks: { $t: (key: string) => key },
    },
  })

describe('MediaGridItem', () => {
  it('renders a plain tile when nothing is selected', () => {
    const wrapper = mountItem({})
    expect(wrapper.find('section.media-item').exists()).toBe(true)
    expect(wrapper.find('section.selected').exists()).toBe(false)
    expect(wrapper.find('section.selecting').exists()).toBe(false)
    expect(wrapper.find('.check-circle.checked').exists()).toBe(false)
  })

  it('marks the tile selected and shows the check', () => {
    const wrapper = mountItem({ selected: true })
    expect(wrapper.find('section.selected').exists()).toBe(true)
    expect(wrapper.find('.check-circle.checked').exists()).toBe(true)
  })

  it('marks shift-affected tiles as selecting and previews the target state', () => {
    const previewSelect = mountItem({ shiftSelected: true, shouldSelect: true })
    expect(previewSelect.find('section.selecting').exists()).toBe(true)
    expect(previewSelect.find('.check-circle.checked').exists()).toBe(true)

    const previewDeselect = mountItem({ selected: true, shiftSelected: true, shouldSelect: false })
    expect(previewDeselect.find('section.selecting').exists()).toBe(true)
    expect(previewDeselect.find('.check-circle.checked').exists()).toBe(false)
  })

  it('shows the zoom button only in select mode', () => {
    expect(mountItem({}).find('.btn-zoom').exists()).toBe(false)
    expect(mountItem({ checked: true }).find('.btn-zoom').exists()).toBe(true)
  })

  it('emits item-click from the tile and toggle-select from the circle', async () => {
    const wrapper = mountItem({ checked: true })

    await wrapper.find('button.icon-button-stub.btn-checkbox').trigger('click')
    expect(wrapper.emitted('toggle-select')).toHaveLength(1)
    expect(wrapper.emitted('item-click')).toBeUndefined()

    await wrapper.find('section').trigger('click')
    expect(wrapper.emitted('item-click')).toHaveLength(1)
  })

  it('renders slot content', () => {
    const wrapper = mount(MediaGridItem, {
      props: {
        item: { id: 'a', tags: [] },
        checked: false,
        selected: false,
        shiftSelected: false,
        shouldSelect: false,
        dataType: DataType.IMAGE,
      },
      global: {
        components: { VIconButton: VIconButtonStub, VCheckCircle: VCheckCircleStub },
        directives: { tooltip: {} },
        mocks: { $t: (key: string) => key },
      },
      slots: { thumbnail: '<img class="thumb" />', 'info-right': '1 MB' },
    })
    expect(wrapper.find('img.thumb').exists()).toBe(true)
    expect(wrapper.find('.info .right').text()).toBe('1 MB')
  })
})
