import { readFileSync } from 'node:fs'
import { parse } from '@vue/compiler-sfc'
import { h, nextTick, ref, type Slots } from 'vue'
import { expect, it, vi } from 'vitest'
import PClipboard from '@/views/clipboard/PClipboard.vue'
import VTextField from '@/components/base/VTextField.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import { click, clientTemplate, content, nodes, renderView } from '../helpers/vue-node-renderer'

const state = vi.hoisted(() => ({ send: vi.fn() }))
const sending = ref(false)
vi.mock('@/stores/main', () => ({ useMainStore: () => ({ quick: 'clipboard' }) }))
vi.mock('@/views/clipboard/clipboard', () => ({ useClipboardData: () => ({
  items: ref([]), listError: ref(''), total: ref(0), page: ref(1), limit: ref(50), loading: ref(false),
  clipboardSync: ref(true), clipText: ref(''), clipTextError: ref(false), setClipLoading: sending,
  sendToPhone: state.send,
}) }))
vi.mock('@/views/clipboard/ClipboardHistory.vue', () => ({ default: { render: () => null } }))
vi.mock('@/components/NoDataPlaceholder.vue', () => ({ default: { render: () => null } }))

clientTemplate(PClipboard, 'src/views/clipboard/PClipboard.vue')
clientTemplate(VFilledButton, 'src/components/base/VFilledButton.vue')
clientTemplate(VTextField, 'src/components/base/VTextField.vue')

it('uses the send-to-phone prompt as the input placeholder without a separate title', () => {
  const button = { setup(_: unknown, { slots }: { slots: Slots }) {
    return () => h('button', slots.default?.())
  } }
  const view = renderView(PClipboard, {}, {
    VTextField, VIconButton: button, VFilledButton: button, VPagination: () => null,
  })
  try {
    const all = nodes(view.root)
    const input = all.find(node => node.tag === 'input')!
    expect(input.props.placeholder).toBe('send_to_phone_clipboard')
    expect(all.some(node => node.tag === 'label' || node.props.class === 'section-title')).toBe(false)
    click(all.find(node => node.tag === 'button' && content(node).trim() === 'send')!)
    expect(state.send).toHaveBeenCalledOnce()
  } finally { view.unmount() }
})

it('keeps a 16px gap above the composer when there is no title', () => {
  const { descriptor } = parse(readFileSync(new URL('../../src/views/clipboard/PClipboard.vue', import.meta.url), 'utf8'))
  expect(descriptor.styles[0]?.content).toMatch(/\.clip-composer\s*\{[^{}]*\bmargin:\s*16px 16px 8px;/)
})

it('keeps the send button label unwrapped and the progress indicator out of its layout', async () => {
  const view = renderView(PClipboard, {}, {
    VTextField, VFilledButton, VIconButton: () => null, VPagination: () => null, VCircularProgress: () => h('span'),
  })
  try {
    const button = nodes(view.root).find(node => node.tag === 'button' && content(node).trim() === 'send')!
    expect(button.props.disabled).toBe(false)
    expect(nodes(button).some(node => node.props.class === 'sm')).toBe(false)
    sending.value = true
    await nextTick()
    expect(button.props.disabled).toBe(true)
    expect(content(button).trim()).toBe('send')
    expect(nodes(button).some(node => node.props.class === 'sm')).toBe(true)
    const { descriptor } = parse(readFileSync(new URL('../../src/views/clipboard/PClipboard.vue', import.meta.url), 'utf8'))
    const styles = descriptor.styles[0]?.content
    expect(styles).toMatch(/\.send-btn\s*\{[^{}]*\bflex:\s*none;/)
    expect(styles).toMatch(/\.send-btn\s*\{[^{}]*\bwhite-space:\s*nowrap;/)
    expect(styles).toMatch(/:deep\(\.v-circular-progress\)\s*\{[^{}]*\bposition:\s*absolute;/)
  } finally {
    view.unmount()
    sending.value = false
  }
})
