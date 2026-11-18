import { afterEach, expect, it, vi } from 'vitest'
import { h } from 'vue'
import ForwardMessageModal from '@/views/chat/ForwardMessageModal.vue'
import { clientTemplate, renderView, nodes, content, click, flush, ModalShell } from '../helpers/vue-node-renderer'

const { request, pop } = vi.hoisted(() => ({ request: vi.fn(), pop: vi.fn() }))
vi.mock('@/components/DeviceTypeIcon.vue', () => ({ default: { render: () => h('span', 'device icon') } }))
vi.mock('@/components/modal', () => ({ closeById: pop, getCurrentModal: () => ({ id: 12 }), modalQueue: { value: [{ id: 12 }] } }))
vi.mock('@/lib/api/gql-client', () => ({ bindGraphqlClient: () => request, gqlFetch: request }))
vi.mock('@/stores/chat', () => ({ useChatStore: () => ({
  joinedChannels: [{ id: 'source', name: 'Origin' }, { id: 'team', name: 'Team' }],
  pairedPeers: [{ id: 'desk', name: 'Desk', deviceType: 'COMPUTER' }, { id: 'phone', name: 'Phone', deviceType: 'PHONE' }],
}) }))

clientTemplate(ForwardMessageModal, 'src/views/chat/ForwardMessageModal.vue')
const unmounts: Array<() => void> = []
afterEach(() => { unmounts.splice(0).forEach(stop => stop()); vi.resetAllMocks(); vi.unstubAllGlobals() })

const button = { setup(_: unknown, { slots }: any) { return () => h('button', slots.default?.()) } }
function mount() {
  const addEventListener = vi.fn()
  vi.stubGlobal('document', { addEventListener, removeEventListener: vi.fn() })
  const view = renderView(ForwardMessageModal, { modalId: '_modal_12', message: { id: 'original' }, excludeChatId: 'channel:source' }, {
    VModal: ModalShell,
    VListItem: { props: ['title', 'subtitle', 'active'], setup(props: any, { slots }: any) {
      return () => h('li', { name: props.title, status: props.subtitle, active: props.active }, [slots.start?.(), slots.end?.()])
    } },
    VCheckbox: { props: ['label', 'checked'], setup(props: any) {
      return () => h('input', { type: 'checkbox', label: props.label, checked: props.checked })
    } },
    VTextField: { props: ['modelValue', 'placeholder'], emits: ['update:modelValue'], setup(props: any, { emit }: any) {
      return () => h('textarea', { value: props.modelValue, placeholder: props.placeholder, onInput: (text: string) => emit('update:modelValue', text) })
    } },
    VFilledButton: button,
    VOutlinedButton: button,
    VTextButton: button,

  })
  unmounts.push(view.unmount)
  const list = () => nodes(view.root).filter(node => node.tag === 'li')
  const row = (name: string) => list().find(node => node.props.name === name)!
  const send = () => nodes(view.root).filter(node => node.tag === 'button').at(-1)!
  const note = () => nodes(view.root).find(node => node.tag === 'textarea')!
  return { ...view, list, row, send, note, addEventListener }
}

const received = (items: Array<{ channelId?: string; toId?: string; status: string }>) => ({ data: { forwardChatItems: items } })

it('renders a flat eligible list and sends the original plus optional note in one batch', async () => {
  let resolve!: (response: unknown) => void
  request.mockReturnValueOnce(new Promise(done => { resolve = done }))
  const view = mount()
  expect(view.list().map(node => node.props.name)).toEqual(['Team', 'Desk', 'Phone'])
  expect(view.note().props.placeholder).toBe('forward_note_optional')
  expect(view.send().props.disabled).toBe(true)
  click(view.row('Team')); click(view.row('Desk')); view.note().props.onInput('  Please read this  ')
  await flush()
  expect(content(view.send())).toBe('forward_send_count')
  click(view.send())
  await flush()
  click(view.send())
  expect(request).toHaveBeenCalledExactlyOnceWith(expect.stringContaining('note: $note'), {
    id: 'original', targets: ['channel:team', 'peer:desk'], note: 'Please read this',
  })
  expect(view.send().props.disabled).toBe(true)
  expect(pop).not.toHaveBeenCalled()
  const stopImmediatePropagation = vi.fn()
  expect(view.addEventListener).toHaveBeenCalledWith('keyup', expect.any(Function), true)
  view.addEventListener.mock.lastCall![1]({ key: 'Escape', stopImmediatePropagation })
  expect(stopImmediatePropagation).toHaveBeenCalledOnce()
  resolve(received([{ channelId: 'team', status: 'SENT' }, { channelId: 'team', status: 'SENT' },
    { toId: 'desk', status: 'PENDING' }, { toId: 'desk', status: 'PENDING' }]))
  await flush()
  expect(pop).toHaveBeenCalledExactlyOnceWith(12)
})

it('keeps the draft and does not recreate either delivered or failed persisted messages', async () => {
  request.mockResolvedValueOnce(received([{ channelId: 'team', status: 'SENT' }, { channelId: 'team', status: 'PARTIAL' },
    { toId: 'desk', status: 'SENT' }, { toId: 'desk', status: 'SENT' }]))
  const view = mount()
  click(view.row('Team')); click(view.row('Desk')); view.note().props.onInput('Keep')
  await flush(); click(view.send()); await flush()
  expect(pop).not.toHaveBeenCalled()
  expect(view.note().props.value).toBe('Keep')
  expect(view.row('Team').props.status).toBe('forward_delivery_failed')
  expect(view.row('Desk').props.status).toBe('forward_submitted')
  click(view.row('Team')); click(view.row('Desk'))
  await flush()
  expect(view.send().props.disabled).toBe(true)
  expect(request).toHaveBeenCalledTimes(1)
})

it('does not repeat an ambiguous batch, and omits whitespace-only notes', async () => {
  request.mockRejectedValueOnce(new Error('Response lost'))
  const view = mount()
  click(view.row('Phone')); view.note().props.onInput('   ')
  await flush(); click(view.send()); await flush()
  expect(request).toHaveBeenCalledExactlyOnceWith(expect.any(String), { id: 'original', targets: ['peer:phone'] })
  expect(pop).not.toHaveBeenCalled()
  expect(view.row('Phone').props.status).toBe('forward_unconfirmed')
  click(view.row('Phone')); await flush(); click(view.send())
  expect(request).toHaveBeenCalledTimes(1)
})
