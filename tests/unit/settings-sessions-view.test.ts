import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { h, ref } from 'vue'
import SettingsSessionsView from '@/views/settings/SettingsSessionsView.vue'
import EditValueModal from '@/components/EditValueModal.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import VFilterChip from '@/components/base/VFilterChip.vue'
import { clientTemplate, renderView, nodes, content, click, flush } from '../helpers/vue-node-renderer'

const fixture = vi.hoisted(() => ({ context: null as any, rows: [] as any[], request: vi.fn(), open: vi.fn(), online: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: (...args: unknown[]) => fixture.request(...args), getGraphqlContextKey: () => fixture.context.value }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/device/sessions', () => ({ subscribeOnlineClients: (...args: unknown[]) => fixture.online(...args) }))
vi.mock('@/components/modal', () => ({ openModal: (...args: unknown[]) => fixture.open(...args), popModal: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key, te: () => false }) }))
vi.mock('@/views/settings/sessions/SessionCard.vue', () => ({ default: { props: { entry: { type: Object, required: true } }, emits: ['rename'], setup(props: any, { emit }: any) { return () => h('button', { onClick: () => emit('rename') }, props.entry.clientId) } } }))
vi.mock('@/views/settings/sessions/CreatedTokenDialog.vue', () => ({ default: { props: { credential: { type: Object, required: true } }, emits: ['close'], setup(props: any, { emit }: any) { return () => h('div', [h('span', props.credential.token), h('button', { onClick: () => emit('close') }, 'close-created')]) } } }))

clientTemplate(SettingsSessionsView, 'src/views/settings/SettingsSessionsView.vue')
clientTemplate(VFilledButton, 'src/components/base/VFilledButton.vue')
clientTemplate(VFilterChip, 'src/components/base/VFilterChip.vue')
const mounted: Array<() => void> = []
beforeEach(() => {
  vi.clearAllMocks()
  fixture.context = ref('peer-a')
  fixture.rows = []
  fixture.open.mockResolvedValue({ closed: ref(false), close: vi.fn().mockResolvedValue(undefined) })
  fixture.request.mockImplementation(async (document: string) => {
    if (document.includes('mutation createApiToken')) return { data: { createApiToken: { clientId: 'fixture-api', token: 'synthetic-secret' } } }
    return { data: { sessions: fixture.rows } }
  })
})
afterEach(() => { mounted.splice(0).forEach(unmount => unmount()) })
function mount() {
  const result = renderView(SettingsSessionsView, {}, {
    VFilledButton, VFilterChip,
    VIconButton: { render: () => null }, VCircularProgress: { render: () => null },
  })
  mounted.push(result.unmount)
  return result.root
}

it('enables creation in a browser/remote view and displays the new token for copying', async () => {
  const root = mount()
  await flush()
  click(nodes(root).find(node => node.tag === 'button' && content(node) === 'session_management.api_tokens')!)
  await flush()
  const create = nodes(root).find(node => node.tag === 'button' && content(node) === 'session_management.create_token')!
  expect(create.props.disabled).toBe(false)
  click(create)
  await flush()
  const [component, props] = fixture.open.mock.calls[0]
  expect(component).toBe(EditValueModal)
  expect(props.value).toBe('')
  expect(props.allowEmpty).toBe(false)
  const mutation = props.mutation()
  const done = vi.fn()
  mutation.onDone(done)
  await mutation.mutate(props.getVariables(' Fixture token '))
  await flush()
  expect(fixture.request).toHaveBeenCalledWith(expect.stringContaining('mutation createApiToken'), { name: 'Fixture token' })
  expect(content(root)).toContain('synthetic-secret')
  click(nodes(root).find(node => node.tag === 'button' && content(node) === 'close-created')!)
  await flush()
  expect(content(root)).not.toContain('synthetic-secret')
  expect(done).toHaveBeenCalledOnce()
  expect(fixture.online).not.toHaveBeenCalled()
  expect(nodes(root).some(node => node.tag === 'h1')).toBe(false)
})

it.each([
  ['Saved name', 'Fixture OS', '1', 'Saved name'],
  ['', 'Fixture OS', '1', 'Fixture OS 1'],
  ['', '', '1', '1'],
])('prefills the shared rename modal like plain-app (%s, %s, %s)', async (name, osName, osVersion, expected) => {
  fixture.rows = [{ clientId: 'fixture-session', clientName: name, type: 'LOGIN', osName, osVersion }]
  const root = mount()
  await flush()
  click(nodes(root).find(node => node.tag === 'button' && content(node) === 'fixture-session')!)
  await flush()
  expect(fixture.open).toHaveBeenCalledWith(EditValueModal, expect.objectContaining({ value: expected, allowEmpty: true, borderless: true }))
})

it('does not submit an old dialog to a newly selected target', async () => {
  const root = mount()
  await flush()
  click(nodes(root).find(node => node.tag === 'button' && content(node) === 'session_management.api_tokens')!)
  await flush()
  click(nodes(root).find(node => node.tag === 'button' && content(node) === 'session_management.create_token')!)
  await flush()
  const props = fixture.open.mock.calls[0][1]
  const mutation = props.mutation()
  mutation.onDone(vi.fn())
  fixture.context.value = 'peer-b'
  await flush()
  await mutation.mutate({ name: 'Fixture token' })
  expect(fixture.request.mock.calls.some(([document]) => document.includes('mutation createApiToken'))).toBe(false)
})
