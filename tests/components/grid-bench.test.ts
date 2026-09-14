import { mount } from '@vue/test-utils'
import { defineComponent, h, withMemo } from 'vue'
import { describe, expect, it } from 'vitest'
import { DataType } from '@/lib/data'
import MediaGridItem from '@/components/media/MediaGridItem.vue'

const N = 500
const M = 250

const makeItems = () =>
  Array.from({ length: N }, (_, i) => ({ id: `id-${i}`, tags: [], fileId: `f-${i}`, size: 1024, duration: 5, path: `p-${i}.jpg` }))

const GridBefore = defineComponent({
  props: {
    items: { type: Array, required: true },
    selectedIds: { type: Array, required: true },
    shiftEffectingIds: { type: Array, required: true },
    shouldSelect: { type: Boolean, default: false },
    checked: { type: Boolean, default: false },
  },
  setup(props) {
    return () =>
      h(
        'div',
        { class: 'media-grid' },
        (props.items as any[]).map((item, i) =>
          h(
            MediaGridItem,
            {
              key: item.id,
              item,
              checked: props.checked,
              selectedIds: props.selectedIds,
              shiftEffectingIds: props.shiftEffectingIds,
              shouldSelect: props.shouldSelect,
              dataType: DataType.IMAGE,
              onItemMouseEnter: () => {},
              onToggleSelect: () => {},
              onView: () => {},
            },
            {
              thumbnail: () => h('img', { src: `u-${i}` }),
              'info-right': () => '1 MB',
            }
          )
        )
      )
  },
})

const GridAfter = defineComponent({
  props: {
    items: { type: Array, required: true },
    selected: { type: Array, required: true },
    shiftSelected: { type: Array, required: true },
    shouldSelect: { type: Boolean, default: false },
    checked: { type: Boolean, default: false },
  },
  setup(props) {
    const cache: any[] = []
    return () =>
      h(
        'div',
        { class: 'media-grid' },
        (props.items as any[]).map((item, i) =>
          withMemo(
            [props.checked, props.selected[i], props.shiftSelected[i], props.shouldSelect, item],
            () =>
              h(
                MediaGridItem,
                {
                  key: item.id,
                  item,
                  checked: props.checked,
                  selected: props.selected[i],
                  shiftSelected: props.shiftSelected[i],
                  shouldSelect: props.shouldSelect,
                  dataType: DataType.IMAGE,
                  onItemMouseEnter: () => {},
                  onToggleSelect: () => {},
                  onView: () => {},
                },
                {
                  thumbnail: () => h('img', { src: `u-${i}` }),
                  'info-right': () => '1 MB',
                }
              ),
            cache,
            i
          )
        )
      )
  },
})

const VIconButtonStub = defineComponent({ setup: (_, { slots }: any) => () => h('button', slots.default?.()) })
const VCheckCircleStub = defineComponent({
  props: { checked: { type: Boolean, default: false } },
  setup: (props: any) => () => h('span', props.checked ? { class: 'checked' } : {}),
})
const globalStubs = (checked = true) => ({
  mocks: { $t: (k: string) => k },
  directives: { tooltip: {} },
  components: { VIconButton: VIconButtonStub, VCheckCircle: VCheckCircleStub },
})

const rounds = (dt: number[]) => `min=${Math.min(...dt).toFixed(1)}ms median=${[...dt].sort((a, b) => a - b)[Math.floor(dt.length / 2)].toFixed(1)}ms`

describe('grid bench', () => {
  it('BEFORE: array.includes props', { timeout: 120_000 }, async () => {
    const items = makeItems()
    const selected = items.slice(0, M).map((i) => i.id)
    const wrapper = mount(GridBefore, { props: { items, selectedIds: selected, shiftEffectingIds: [], checked: true }, global: globalStubs() })
    const mountMs = 'excluded (warmup)'
    const dt: number[] = []
    for (let r = 0; r < 5; r++) {
      const next = r % 2 === 0 ? [`${items[0].id}`, ...selected.slice(1)] : selected
      const t0 = performance.now()
      await wrapper.setProps({ selectedIds: next })
      dt.push(performance.now() - t0)
    }
    console.log(`[BEFORE] mount=${mountMs} re-render per toggle: ${rounds(dt)}`)
    expect(dt.length).toBe(5)
  })

  it('AFTER: boolean props + v-memo', { timeout: 120_000 }, async () => {
    const items = makeItems()
    const selected = items.slice(0, M).map(() => true)
    const wrapper = mount(GridAfter, { props: { items, selected, shiftSelected: [], checked: true }, global: globalStubs() })
    const dt: number[] = []
    for (let r = 0; r < 5; r++) {
      const next = r % 2 === 0 ? [false, ...selected.slice(1)] : selected
      const t0 = performance.now()
      await wrapper.setProps({ selected: next })
      dt.push(performance.now() - t0)
    }
    console.log(`[AFTER] re-render per toggle: ${rounds(dt)}`)
    expect(dt.length).toBe(5)
  })
})
