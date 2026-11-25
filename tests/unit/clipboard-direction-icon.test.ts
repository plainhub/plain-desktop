import { expect, it, vi } from 'vitest'
import type { Component, PropType } from 'vue'
import LocalClipboard from '@/views/clipboard/LocalClipboard.vue'
import type { ClipboardDirection } from '@/lib/peer/local-clipboard-data'
import { clientTemplate, flush, nodes, renderView } from '../helpers/vue-node-renderer'

const state = vi.hoisted(() => ({
  direction: vi.fn<() => ClipboardDirection>(),
  refreshDirection: undefined as undefined | ((direction: ClipboardDirection) => void),
}))
vi.mock('@/lib/peer/local-clipboard-data', async () => {
  const { ref } = await import('vue')
  const direction = ref<ClipboardDirection>('OFF')
  state.refreshDirection = value => { direction.value = value }
  state.direction.mockImplementation(() => direction.value)
  return { peerClipboardDirection: state.direction, isClipboardDirection: vi.fn(), setPeerClipboardDirection: vi.fn() }
})
vi.mock('@/stores/main', () => ({ useMainStore: () => ({ quick: 'clipboard' }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/components/NoDataPlaceholder.vue', () => ({ default: { render: () => null } }))
vi.mock('@/views/clipboard/ClipboardHistory.vue', () => ({ default: { render: () => null } }))
vi.mock('@/views/clipboard/local-clipboard', async () => {
  const { ref } = await import('vue')
  return { useLocalClipboard: () => ({
    peerId: ref('fixture-device'), clipboardPeers: ref([{id:'fixture-device',name:'Fixture'}]),
    items: ref([]), status: ref('ready'), total: ref(0), limit: ref(20), page: ref(0),
    gotoPage: vi.fn(), changePageSize: vi.fn(), deleteItem: vi.fn(),
  }) }
})
vi.mock('@/views/clipboard/master-switch', async () => {
  const { ref } = await import('vue')
  return { useClipboardMasterSwitch: () => ({ enabled: ref(true), saving: ref(false), setEnabled: vi.fn() }) }
})
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/base/VChipSelect.vue', async () => {
  const { defineComponent, h } = await import('vue')
  return { default: defineComponent({
    props: { icon: Object as PropType<Component>, iconOnly: Boolean, modelValue: String },
    setup(props) { return () => h('button', {'data-direction': props.iconOnly ? props.modelValue : undefined}, props.icon ? [h(props.icon)] : []) },
  }) }
})

clientTemplate(LocalClipboard, 'src/views/clipboard/LocalClipboard.vue')

it('shows a different toolbar icon for each saved clipboard direction', async () => {
  const view = renderView(LocalClipboard, {}, { 'v-switch': { render: () => null }, 'v-pagination': { render: () => null } })
  try {
    const paths = new Set<string>()
    for (const direction of ['OFF', 'SEND', 'RECEIVE', 'BOTH'] as const) {
      state.refreshDirection!(direction)
      await flush()
      const button = nodes(view.root).find(node => node.props['data-direction'] === direction)!
      const svg = nodes(button).find(node => node.tag === 'svg')!
      if (direction === 'OFF') expect(nodes(svg).some(node => node.tag === 'line')).toBe(true)
      paths.add(nodes(svg).filter(node => node.tag === 'path' || node.tag === 'line').map(node => JSON.stringify(node.props)).join(''))
    }
    expect(paths.size).toBe(4)
  } finally { view.unmount() }
})
