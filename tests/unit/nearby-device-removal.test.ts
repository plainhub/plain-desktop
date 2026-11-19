import { afterEach, expect, it, vi } from 'vitest'
import { createRenderer, h, nextTick, ssrContextKey } from 'vue'
import * as Vue from 'vue'
import { compileScript, compileTemplate, parse } from 'vue/compiler-sfc'
import { createI18n } from 'vue-i18n'
import { transpile } from 'typescript'
import NearbyView from '@/views/chat/NearbyView.vue'
import nearbyViewSource from '@/views/chat/NearbyView.vue?raw'
import DeviceDiscoveryStatus from '@/components/DeviceDiscoveryStatus.vue'
import discoveryStatusSource from '@/components/DeviceDiscoveryStatus.vue?raw'
import VModal from '@/components/base/VModal.vue'
import modalSource from '@/components/base/VModal.vue?raw'
import { pushModal } from '@/components/modal/methods'
import initialize from '@/components/modal/utils/initialize'
import { saveInstance } from '@/components/modal/utils/instances'
import NearbyDeviceItem from '@/views/chat/NearbyDeviceItem.vue'
import nearbyDeviceItemSource from '@/views/chat/NearbyDeviceItem.vue?raw'
import VListItem from '@/components/base/VListItem.vue'
import InlineDeleteConfirm from '@/components/base/InlineDeleteConfirm.vue'
import inlineSource from '@/components/base/InlineDeleteConfirm.vue?raw'
import listItemSource from '@/components/base/VListItem.vue?raw'
import { unpairPeerGQL } from '@/lib/api/mutation'
import { peersGQL } from '@/lib/api/query'
import { DeviceType, PeerStatus } from '@/lib/status'
import { DiscoveryStatus, type DiscoveredDevice } from '@/hooks/use-device-discovery'
import type { SelfDevice } from '@/lib/device/self-device'
import common from '@/locales/en-US/common'
import messages from '@/locales/en-US'
import type { PairingView } from '@/lib/device/pairing'
import RequestApproval from '@/components/RequestApproval.vue'
import RequestApprovalSource from '@/components/RequestApproval.vue?raw'
import RequestVerificationCode from '@/components/RequestVerificationCode.vue'
import requestVerificationSource from '@/components/RequestVerificationCode.vue?raw'

const io = vi.hoisted(() => ({ request: vi.fn(), probe: vi.fn(), pushModal: vi.fn(), removePref: vi.fn(), clearRemote: vi.fn(), setRemote: vi.fn(), start: vi.fn(), current: '', status: 0, devices: [] as DiscoveredDevice[], self: null as SelfDevice | null }))
const bluetooth = vi.hoisted(() => ({ invoke: vi.fn(), supported: false }))
const chatPairing = vi.hoisted(() => ({ start: vi.fn(), cancel: vi.fn(), updates: new Map<string, (state: PairingView) => void>(), update(state: PairingView) { this.updates.get(state.deviceId)?.(state) } }))
const managedPairing = vi.hoisted(() => ({ finish: new Map<string, () => void>(), cancel: vi.fn() }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: { capabilities: bluetooth.supported ? ['BLE'] : [] } }) }))
vi.mock('@/lib/api/native-invoke', () => ({
  invoke: bluetooth.invoke,
  onNativeReload: () => () => {},
  Channel: class {
    id = 17
    constructor(readonly onmessage: (status: string) => void) {}
  },
}))
vi.mock('@/components/modal', async () => ({ popModal: (await import('@/components/modal/methods')).popModal, pushModal: io.pushModal }))
vi.mock('@/lib/api/managed-proxy', () => ({ managedGraphql: io.probe }))
vi.mock('@/lib/api/gql-client', () => ({ nativeGqlFetch: io.request, bindGraphqlClient: () => io.request, gqlFetch: io.request, GqlError: Error }))
vi.mock('@/lib/api/subscription-client', () => ({
  graphqlSubscriptions: { subscribe: () => () => {}, onAttached: () => () => {} },
  SubscriptionClient: class {
    subscribe = () => () => {}
    onAttached = () => () => {}
    setOperations() {}
    start() {}
    stop() {}
  },
}))
vi.mock('@/lib/api/subscription-connection', () => ({ openNative: vi.fn() }))
vi.mock('@/lib/device/client-id', () => ({
  getRemoteClientId: () => io.current,
  clearRemoteClientId: io.clearRemote,
  setRemoteClientId: io.setRemote,
}))
vi.mock('@/lib/device/login-peers', async () => ({
  loginPeers: (await import('vue')).ref([]),
  findLoginPeer: (id: string) => id === io.current ? { id } : undefined,
  preloadLoginPeers: async () => {},
}))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => !io.current }))
vi.mock('@/lib/device/self-device', () => ({ loadSelfDevice: async () => io.self }))
vi.mock('@/lib/prefs/browser', () => ({ remove: io.removePref }))
vi.mock('@/lib/device/pairing', async () => {
  const { ref } = await import('vue')
  return { pairingViews: new Map(), createPairing: () => {
    const state = ref<PairingView | null>(null)
    return { state, start: (input: { peerId: string }) => {
      chatPairing.updates.set(input.peerId, (next) => { state.value = next })
      return chatPairing.start(input)
    }, cancel: chatPairing.cancel, dispose() {} }
  } }
})
vi.mock('@/hooks/use-device-discovery', async () => {
  const { ref } = await import('vue')
  return {
    DiscoveryStatus: { IDLE: 0, SEARCHING: 1, OK: 2, PERMISSION_DENIED: 3, NETWORK_ERROR: 4 },
    useDeviceDiscovery: () => ({
      status: ref(io.status), devices: ref(io.devices), pairingViews: new Map(), start: io.start, stop: async () => {},
    }),
  }
})
vi.mock('@/components/MdnsFirewallFix.vue', () => ({ default: () => null }))
vi.mock('@/views/login/DiscoveredDeviceLogin.vue', () => ({
  default: { props: ['deviceId'], setup(props, { emit, expose, slots }) {
    managedPairing.finish.set(props.deviceId, () => emit('success'))
    const cancel = () => { managedPairing.cancel(props.deviceId); emit('cancel') }
    expose({ cancel, hasCancel: true })
    return () => h('div', [slots.default?.(), h('button', { onClick: cancel }, 'Cancel')])
  } },
}))
vi.mock('@/views/chat/NearbyPairOptions.vue', () => ({
  default: (_, { emit }) => [
    h('button', { onClick: () => emit('confirm', 'chat') }, 'Chat only'),
    h('button', { onClick: () => emit('confirm', 'control') }, 'Full Control'),
  ],
}))

const peer = { id: 'remote', name: 'Test device', deviceType: DeviceType.PHONE, status: PeerStatus.PAIRED, online: false }
const discovered: DiscoveredDevice = { id: 'unpaired', name: 'Discovered test device', deviceType: DeviceType.PHONE,
  status: PeerStatus.UNPAIRED, ips: [], remoteAccess: true, port: 8443, version: '', platform: '', lastSeen: '', discoveryMethods: ['LAN'] }
const self: SelfDevice = { id: 'self', name: 'Local test device', deviceType: DeviceType.COMPUTER, ips: [], host: '', port: 8443, publicKey: '' }

// Vitest compiles Vue templates for SSR. The click tests need the client render functions.
function clientRender(source: string) {
  const { descriptor } = parse(source)
  const { code, errors } = compileTemplate({
    source: descriptor.template!.content,
    filename: 'nearby-device-removal.vue',
    id: 'nearby-device-removal',
    compilerOptions: {
      mode: 'function',
      prefixIdentifiers: true,
      bindingMetadata: compileScript(descriptor, { id: 'nearby-device-removal' }).bindings,
      isCustomElement: (tag) => tag.startsWith('i-material-symbols:') || tag.startsWith('i-lucide:'),
    },
  })
  if (errors.length) throw new Error(errors.join('\n'))
  return new Function('Vue', transpile(code))(Vue)
}
NearbyView.render = clientRender(nearbyViewSource)
DeviceDiscoveryStatus.render = clientRender(discoveryStatusSource)
VModal.render = clientRender(modalSource)
NearbyDeviceItem.render = clientRender(nearbyDeviceItemSource)
VListItem.render = clientRender(listItemSource)
InlineDeleteConfirm.render = clientRender(inlineSource)
RequestApproval.render = clientRender(RequestApprovalSource)
RequestVerificationCode.render = clientRender(requestVerificationSource)

type Node = { text: string; children: Node[]; parent: Node | null; props: Record<string, unknown>; style: { display?: string } }

const makeNode = (text = ''): Node => ({ text, children: [], parent: null, props: {}, style: {} })
function removeNode(node: Node) {
  const siblings = node.parent?.children
  if (siblings) siblings.splice(siblings.indexOf(node), 1)
  node.parent = null
}
const renderer = createRenderer<Node, Node>({
  createElement: () => makeNode(),
  createText: makeNode,
  createComment: () => makeNode(),
  setText: (node, text) => { node.text = text },
  setElementText: (node, text) => { node.text = text },
  parentNode: (node) => node.parent,
  nextSibling: (node) => node.parent?.children[node.parent.children.indexOf(node) + 1] ?? null,
  patchProp: (node, key, _, value) => { node.props[key] = value },
  querySelector: () => document.body as unknown as Node,
  insert(node, parent, anchor) {
    removeNode(node)
    node.parent = parent
    parent.children.splice(anchor ? parent.children.indexOf(anchor) : parent.children.length, 0, node)
  },
  remove: removeNode,
})

function nodes(node: Node): Node[] {
  return node.style.display === 'none' ? [] : [node, ...node.children.flatMap(nodes)]
}

let unmount: (() => void) | undefined
afterEach(async () => {
  unmount?.()
  await Promise.resolve()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
  vi.resetAllMocks()
  chatPairing.updates.clear()
  managedPairing.finish.clear()
  bluetooth.supported = false
})

async function mount(current = '', fail = false, fixtures: { paired?: boolean; devices?: DiscoveredDevice[]; self?: SelfDevice; switchOnly?: boolean; online?: boolean; status?: DiscoveryStatus } = {}) {
  io.current = current
  io.devices = fixtures.devices ?? []
  io.status = fixtures.status ?? DiscoveryStatus.SEARCHING
  io.self = fixtures.self ?? null
  const location = { href: '/nearby?q=keep#hash', protocol: 'https:', reload: vi.fn() }
  vi.stubGlobal('window', { location })
  vi.stubGlobal('__IS_TAURI__', !fixtures.switchOnly)
  let paired = fixtures.paired ?? true
  io.request.mockImplementation(async (document: string) => {
    if (document === peersGQL) return { data: { peers: [{ ...peer, online: fixtures.online ?? (fixtures.switchOnly ? true : peer.online), managed: fixtures.switchOnly && paired, status: paired ? PeerStatus.PAIRED : PeerStatus.UNPAIRED }] } }
    if (document !== unpairPeerGQL) throw new Error('Unexpected GraphQL request')
    if (fail) return { errors: [{ message: 'Device unavailable' }] }
    paired = false
    return { data: { unpairPeer: true } }
  })
  io.probe.mockResolvedValue({ data: { app: { clientId: peer.id } } })
  const body = Object.assign(makeNode(), { style: {} })
  vi.stubGlobal('document', Object.assign(new EventTarget(), { body }))
  initialize()
  const modal = await pushModal(NearbyView)
  const app = renderer.createApp({
    render: () => modal.closed.value ? null : h(NearbyView, {
      switchOnly: fixtures.switchOnly, ref: (instance) => { if (instance) saveInstance(modal.id, instance) },
    }),
  })
  app.provide(ssrContextKey, { modules: new Set() })
  app.use(createI18n({ legacy: false, locale: 'en-US', messages: { 'en-US': messages } }))
  app.component('VListItem', VListItem)
  app.component('InlineDeleteConfirm', InlineDeleteConfirm)
  app.component('VModal', VModal)
  app.component('VCircularProgress', () => h('span', { role: 'progressbar' }))
  app.component('VDropdownMenu', (props, { slots }) => props.modelValue ? slots.default?.() : null)
  app.component('VDropdown', (_, { slots }) => h('div', slots.trigger?.()))
  for (const name of ['VIconButton', 'VOutlinedButton', 'VFilledButton', 'VTextButton']) {
    app.component(name, { setup: (_, { slots }) => () => h('button', slots.default?.()) })
  }
  app.component('PeerIcon', () => null)
  app.component('TouchPhone', () => h('svg'))
  const root = makeNode()
  app.mount(root)
  unmount = () => { app.unmount(); if (!modal.closed.value) void modal.close() }
  await new Promise<void>((resolve) => setImmediate(resolve))
  const click = async (match: (node: Node) => boolean) => {
    const button = nodes(body).find((node) => typeof node.props.onClick === 'function' && match(node))
    expect(button).toBeDefined()
    expect(button?.props.disabled).not.toBe(true)
    const onClick = button!.props.onClick as (event: unknown) => void
    onClick({ stopPropagation() {} })
    await nextTick()
  }
  const clickText = (label: string) => click((node) => node.props['aria-label'] === label || node.children.some((child) => child.text === label) || node.text === label)
  const openRemoval = async () => {
    await click((node) => node.props.id === 'nearby-actions-remote')
    expect(nodes(body).filter((node) => node.props.class === 'dropdown-item').map((node) => node.text)).toContain('Unpair')
    await clickText('Unpair')
  }
  const escape = async () => {
    for (const type of ['keydown', 'keyup']) document.dispatchEvent(Object.assign(new Event(type), { key: 'Escape' }))
    await new Promise<void>((resolve) => setImmediate(resolve))
  }
  return { location, openRemoval, openActions: () => click(node => node.props.id === 'nearby-actions-remote'), clickText, escape, deviceLists: () => nodes(body).filter((node) => node.props.class === 'card list-items').length,
    text: () => nodes(body).map((node) => node.text || node.props['aria-label'] || '').join(' '), searching: () => nodes(body).some((node) => node.props.role === 'progressbar'),
    hasBackButton: () => nodes(body).some((node) => node.props['aria-label'] === 'Back') }
}

it.each([undefined, self])('does not show the paired heading with no paired peer (self: %j)', async (local) => {
  const view = await mount('', false, { paired: false, devices: [discovered], self: local })
  expect(view.text()).not.toContain('Paired devices')
  expect(view.text()).toContain(discovered.name)
  expect(view.text().includes(self.name)).toBe(Boolean(local))
  expect(view.deviceLists()).toBe(local ? 2 : 1)
})

it('shows the paired heading and keeps the offline paired peer visible', async () => {
  const view = await mount('', false, { devices: [discovered] })
  expect(view.text()).toContain('Paired devices')
  expect(view.text()).toContain(peer.name)
  expect(view.text()).toContain(discovered.name)
  expect(view.deviceLists()).toBe(2)
})

it.each([
  ['denied', 'Searching on your local network only.'],
  ['off', 'Bluetooth is turned off'],
  ['unsupported', 'Bluetooth is unavailable'],
])('requests Bluetooth only on click and preserves LAN search when %s', async (status, message) => {
  bluetooth.supported = true
  bluetooth.invoke.mockImplementation(async (command, options) => {
    if (command === 'nearby_bluetooth_enable') options.states.onmessage(status)
  })
  const view = await mount('', false, { paired: false, devices: [discovered] })
  expect(view.text()).toMatch(/Discovered test device.*Enable Bluetooth/)
  expect(bluetooth.invoke).not.toHaveBeenCalled()
  expect(io.start).toHaveBeenCalledOnce()
  await view.clickText('Enable Bluetooth')
  expect(bluetooth.invoke).toHaveBeenCalledExactlyOnceWith('nearby_bluetooth_enable', { states: expect.objectContaining({ id: 17 }) })
  expect(view.text()).toContain(message)
  expect(view.text()).toContain(discovered.name)
  if (status === 'denied') {
    await view.clickText('Allow Bluetooth in Settings')
    expect(bluetooth.invoke).toHaveBeenLastCalledWith('nearby_bluetooth_open_settings')
  }
  await view.clickText('Close')
  expect(bluetooth.invoke).toHaveBeenLastCalledWith('nearby_bluetooth_stop', { scope: 17 })
})

it('returns from Full Control confirmation on Escape through the form cancellation', async () => {
  const view = await mount('', false, { paired: false, devices: [{ ...discovered, ips: ['192.0.2.10'] }] })
  await view.clickText('Pair')
  expect(view.hasBackButton()).toBe(true)
  await view.clickText('Full Control')
  expect(view.text()).toContain('Cancel')
  expect(view.hasBackButton()).toBe(false)
  await view.escape()
  expect(view.text()).toContain(discovered.name)
  expect(view.deviceLists()).toBe(1)
})

it('keeps a Chat only request pending while starting a request for another device', async () => {
  const waiting = { deviceId: discovered.id, phase: 'PAIRING', verificationCode: '01234567', error: null } as const
  chatPairing.start.mockImplementation(async ({ peerId }) => chatPairing.update({ ...waiting, deviceId: peerId }))
  chatPairing.cancel.mockImplementation(async (id) => chatPairing.update({ ...waiting, deviceId: id, phase: 'CANCELED' }))
  const other = { ...discovered, id: 'other', name: 'Second test device' }
  const view = await mount('', false, { paired: false, devices: [discovered, other] })
  await view.clickText('Pair')
  await view.clickText('Chat only')
  await view.clickText('Pair another device')
  expect(chatPairing.cancel).not.toHaveBeenCalled()
  expect(view.text()).toContain('Waiting for confirmation')
  await view.clickText('Pair')
  await view.clickText('Chat only')
  expect(chatPairing.start.mock.calls.map(([input]) => input.peerId)).toEqual([discovered.id, other.id])
  await view.escape()
  expect(chatPairing.cancel).toHaveBeenCalledExactlyOnceWith(other.id)
  expect(view.text()).toContain('Waiting for confirmation')
  await view.clickText('Cancel')
  expect(chatPairing.cancel).toHaveBeenLastCalledWith(discovered.id)
  expect(view.text()).not.toContain('Waiting for confirmation')
})

it('keeps a Full Control request active and does not interrupt the next confirmation when it succeeds', async () => {
  const managed = { ...discovered, ips: ['192.0.2.10'] }
  const other = { ...discovered, id: 'other', name: 'Second test device' }
  const waiting = { deviceId: other.id, phase: 'PAIRING', verificationCode: '01234567', error: null } as const
  chatPairing.start.mockImplementation(async () => chatPairing.update(waiting))
  chatPairing.cancel.mockImplementation(async () => chatPairing.update({ ...waiting, phase: 'CANCELED' }))
  const view = await mount('', false, { paired: false, devices: [managed, other] })
  await view.clickText('Pair')
  await view.clickText('Full Control')
  await view.clickText('Pair another device')
  expect(managedPairing.cancel).not.toHaveBeenCalled()
  expect(view.text()).toContain('Waiting for confirmation')
  await view.clickText('Pair')
  await view.clickText('Chat only')
  managedPairing.finish.get(managed.id)!()
  await nextTick()
  expect(view.text()).toContain('01234567')
  expect(view.location.href).toBe('/nearby?q=keep#hash')
  expect(io.setRemote).not.toHaveBeenCalled()
  await view.escape()
  expect(view.deviceLists()).toBe(1)
  expect(view.text()).not.toContain('Waiting for confirmation')
})

it('does not offer Bluetooth on a host without the capability', async () => {
  const view = await mount('', false, { paired: false, devices: [discovered] })
  expect(view.text()).toContain(discovered.name)
  expect(view.text()).not.toContain('Enable Bluetooth')
  expect(bluetooth.invoke).not.toHaveBeenCalled()
})

it.each(['Cancel', 'Escape'] as const)('returns from Chat only confirmation via %s and only closes from the list', async (action) => {
  const waiting = { deviceId: discovered.id, phase: 'PAIRING', verificationCode: '01234567', error: null } as const
  chatPairing.start.mockImplementation(async () => chatPairing.update(waiting))
  chatPairing.cancel.mockImplementation(async () => chatPairing.update({ ...waiting, phase: 'CANCELED' }))
  const view = await mount('', false, { paired: false, devices: [discovered] })
  await view.clickText('Pair')
  await view.clickText('Chat only')
  expect(chatPairing.start).toHaveBeenCalledExactlyOnceWith({ peerId: discovered.id, mode: 'CHAT' })
  expect(view.deviceLists()).toBe(0)
  expect(view.text()).toContain('01234567')
  const hadBackButton = view.hasBackButton()
  if (action === 'Escape') await view.escape()
  else await view.clickText('Cancel')
  expect(chatPairing.cancel).toHaveBeenCalledExactlyOnceWith(discovered.id)
  expect(view.deviceLists()).toBe(1)
  expect(view.text()).not.toContain('01234567')
  await view.escape()
  expect(view.deviceLists()).toBe(0)
  expect(hadBackButton).toBe(false)
})

it.each([
  [DiscoveryStatus.SEARCHING, false, true],
  [DiscoveryStatus.SEARCHING, true, true],
  [DiscoveryStatus.OK, true, true],
  [DiscoveryStatus.NETWORK_ERROR, false, false],
  [DiscoveryStatus.PERMISSION_DENIED, false, false],
] as const)('keeps searching while discovery is active, including with results (status %s, results %s)', async (status, hasResults, searching) => {
  const view = await mount('', false, { paired: false, status, devices: hasResults ? [discovered] : [] })
  expect(view.searching()).toBe(searching)
  if (status === DiscoveryStatus.SEARCHING || status === DiscoveryStatus.OK) {
    expect(view.text()).toContain('Searching for nearby PlainApp devices…')
  } else {
    expect(view.text()).not.toContain('Searching for nearby PlainApp devices…')
  }
  if (status === DiscoveryStatus.NETWORK_ERROR) expect(view.text()).toContain('Unable to scan nearby devices')
})

it.each(['chat', 'control'] as const)('keeps Nearby open and the current device selected after %s pairing succeeds', async (mode) => {
  const waiting = { deviceId: discovered.id, phase: 'PAIRING', verificationCode: '01234567', error: null } as const
  chatPairing.start.mockImplementation(async () => chatPairing.update(waiting))
  const view = await mount('', false, { paired: false, devices: [{ ...discovered, ips: ['192.0.2.10'] }] })
  await view.clickText('Pair')
  await view.clickText(mode === 'chat' ? 'Chat only' : 'Full Control')
  if (mode === 'chat') chatPairing.update({ ...waiting, phase: 'SUCCEEDED' })
  else managedPairing.finish.get(discovered.id)!()
  await new Promise<void>(resolve => setImmediate(resolve))
  expect(view.deviceLists()).toBe(1)
  expect(view.text()).toContain('Nearby Devices')
  expect(view.location.href).toBe('/nearby?q=keep#hash')
  expect(view.location.reload).not.toHaveBeenCalled()
  expect(io.setRemote).not.toHaveBeenCalled()
  expect(io.clearRemote).not.toHaveBeenCalled()
})

it('shows the pairing failure without offering a discovery retry as a pairing action', async () => {
  const waiting = { deviceId: discovered.id, phase: 'PAIRING', verificationCode: '01234567', error: null } as const
  chatPairing.start.mockImplementation(async () => chatPairing.update(waiting))
  const view = await mount('', false, { paired: false, devices: [discovered] })
  await view.clickText('Pair')
  await view.clickText('Chat only')
  chatPairing.update({ ...waiting, phase: 'FAILED' })
  await nextTick()
  expect(view.text()).toContain('Pairing failed')
  expect(view.text()).not.toContain('Retry')
  expect(view.deviceLists()).toBe(1)
  expect(chatPairing.start).toHaveBeenCalledOnce()
})

it('shows one removal action and lets the user cancel before any mutation', async () => {
  const view = await mount(peer.id)
  expect(view.text()).toContain('Paired devices')
  await view.openRemoval()
  expect(view.text()).toContain('Unpair “Test device”?')
  expect(view.text()).toContain('You’ll switch back to this computer.')
  await view.clickText('Cancel')
  expect(view.text()).toContain('Test device')
  expect(io.request.mock.calls.every(([document]) => document === peersGQL)).toBe(true)
  expect(io.clearRemote).not.toHaveBeenCalled()
})

it.each(['', peer.id])('removes the device only after confirmation (current device: %s)', async (current) => {
  const view = await mount(current)
  await view.openRemoval()
  expect(view.text()).toContain('Your chat history is kept.')
  expect(view.text().includes('switch back to this computer')).toBe(Boolean(current))
  await view.clickText('Unpair')
  await new Promise<void>((resolve) => setImmediate(resolve))
  expect(io.request.mock.calls.filter(([document]) => document === unpairPeerGQL)).toEqual([[unpairPeerGQL, { peerId: peer.id }]])
  expect(io.removePref).toHaveBeenCalledWith('main_state:remote')
  expect(io.clearRemote).toHaveBeenCalledTimes(current ? 1 : 0)
  expect(view.location.href).toBe(current ? '/' : '/nearby?q=keep#hash')
  if (!current) {
    expect(view.text()).not.toContain('Test device')
    expect(view.text()).not.toContain('Paired devices')
  }
})

it('shows a removal error and leaves the current session available for retry', async () => {
  vi.spyOn(console, 'error').mockImplementation(() => {})
  const view = await mount(peer.id, true)
  await view.openRemoval()
  await view.clickText('Unpair')
  expect(view.text()).toContain('Unpair “Test device”?')
  expect(view.text()).toContain(common.NETWORK_ERROR)
  expect(io.removePref).not.toHaveBeenCalled()
  expect(io.clearRemote).not.toHaveBeenCalled()
})


it.each(['', 'remote'])('switches the managed Web picker without changing the visible URL (%s)', async (current) => {
  const view = await mount(current, false, { switchOnly: true, self })
  expect(view.text()).toContain('Switch Device')
  expect(io.start).not.toHaveBeenCalled()
  await view.clickText('Switch')
  expect(view.location.href).toBe('/nearby?q=keep#hash')
  expect(view.location.reload).toHaveBeenCalledOnce()
  if (current) expect(io.clearRemote).toHaveBeenCalledOnce()
  else expect(io.setRemote).toHaveBeenCalledExactlyOnceWith(peer.id)
  expect(io.request.mock.calls.every(([document]) => document === peersGQL)).toBe(true)
})

it('checks a stale offline Web row on explicit Switch and does not change target on failed preflight', async () => {
  const view = await mount('', false, { switchOnly: true, online: false, self })
  let reject!: (reason: Error) => void
  io.probe.mockReturnValue(new Promise((_, fail) => { reject = fail }))
  await view.clickText('Switch')
  expect(io.probe).toHaveBeenCalledOnce()
  expect(view.text()).toContain('Switching…')
  expect(io.setRemote).not.toHaveBeenCalled()
  expect(view.location.reload).not.toHaveBeenCalled()
  const { GqlError } = await import('@/lib/api/gql-error')
  reject(new GqlError('NETWORK_ERROR', 502))
  await vi.waitFor(() => expect(io.pushModal).toHaveBeenCalledOnce())
  expect(io.pushModal.mock.calls[0]?.[1]).toEqual({ peerName: peer.name, originName: self.name })
  expect(view.text()).not.toContain('Switching…')
  expect(io.setRemote).not.toHaveBeenCalled()
  expect(view.location.reload).not.toHaveBeenCalled()
})

it('exposes Unpair for a stale managed row in the Web picker without requiring a connection', async () => {
  const view = await mount('', false, { switchOnly: true, online: false, self })
  await view.openActions()
  await view.clickText('Unpair')
  expect(view.text()).toContain('Unpair “Test device”?')
  expect(io.request.mock.calls.every(([document]) => document === peersGQL)).toBe(true)
  expect(io.probe).not.toHaveBeenCalled()
})

it('confirms and removes a stale saved Web peer through the gateway using the declared peerId variable', async () => {
  const view = await mount('', false, { switchOnly: true, online: false, self })
  await view.openActions()
  await view.clickText('Unpair')
  expect(view.text()).toContain('Your chat history is kept.')
  await view.clickText('Cancel')
  expect(io.request.mock.calls.every(([document]) => document === peersGQL)).toBe(true)
  await view.openActions()
  await view.clickText('Unpair')
  await view.clickText('Confirm')
  await new Promise<void>(done => setImmediate(done))
  expect(io.request.mock.calls.filter(([document]) => document === unpairPeerGQL)).toEqual([[unpairPeerGQL, { peerId: peer.id }]])
  expect(view.text()).not.toContain(peer.name)
  expect(io.probe).not.toHaveBeenCalled()
})

it.each(
  [false, true].flatMap(fullControl => [false, true].flatMap(current => [false, true].flatMap(pending =>
    [false, true].flatMap(online => [false, true].map(switchOnly => ({ fullControl, current, pending, online, switchOnly }))))))
)('enforces paired-row action states: %j', async (state) => {
  const connect = vi.fn(), cancel = vi.fn(), remove = vi.fn()
  const root = makeNode()
  const app = renderer.createApp({ render: () => h(NearbyDeviceItem, {
    peer: { ...peer, online: state.online }, device: { ...discovered, id: peer.id }, ...state,
    onConnect: connect, onCancel: cancel, onRemove: remove,
  }) })
  app.provide(ssrContextKey, { modules: new Set() })
  app.use(createI18n({ legacy: false, locale: 'en-US', messages: { 'en-US': messages } }))
  app.component('VListItem', VListItem)
  app.component('VDropdownMenu', (props, { slots }) => props.modelValue ? slots.default?.() : null)
  app.component('VDropdown', (_, { slots }) => slots.trigger?.())
  app.component('PeerIcon', () => null)
  for (const name of ['VIconButton', 'VOutlinedButton', 'VTextButton']) {
    app.component(name, { setup: (_, { slots }) => () => h('button', { 'data-control': name }, slots.default?.()) })
  }
  app.mount(root)
  unmount = () => app.unmount()
  const controls = () => nodes(root).filter(node => node.props['data-control'])
  const switchVisible = state.fullControl && !state.current && !state.pending
  expect(controls().map(node => node.props['data-control'])).toEqual(
    state.pending ? ['VOutlinedButton'] : switchVisible ? ['VIconButton', 'VIconButton'] : ['VIconButton']
  )
  expect(connect).not.toHaveBeenCalled()
  expect(cancel).not.toHaveBeenCalled()
  expect(remove).not.toHaveBeenCalled()
  const click = async (node: Node) => {
    ;(node.props.onClick as (event: unknown) => void)({ stopPropagation() {} })
    await nextTick()
  }
  if (state.pending) {
    expect(nodes(root).some(node => node.props.id === 'nearby-actions-remote')).toBe(false)
    await click(controls()[0]!)
    expect(cancel).toHaveBeenCalledOnce()
    return
  }
  if (switchVisible) {
    const button = controls()[0]!
    expect(button.props['aria-label']).toBe('Switch')
    expect(button.props.title).toBe('Switch')
    expect(Boolean(button.props.disabled)).toBe(!state.online && !state.switchOnly)
    if (!button.props.disabled) {
      await click(button)
      expect(connect).toHaveBeenCalledExactlyOnceWith('switch')
    }
  }
  const more = controls().find(node => node.props.id === 'nearby-actions-remote')!
  await click(more)
  const items = nodes(root).filter(node => String(node.props.class).includes('dropdown-item'))
  const showControl = !state.fullControl && !state.current
  expect(items.map(node => node.text || node.children.map(child => child.text).join(''))).toEqual(showControl ? ['Full control', 'Unpair'] : ['Unpair'])
  if (showControl) {
    expect(Boolean(items[0]!.props.disabled)).toBe(!state.online)
    if (state.online) {
      await click(items[0]!)
      expect(connect).toHaveBeenCalledExactlyOnceWith('control')
      expect(nodes(root).some(node => String(node.props.class).includes('dropdown-item'))).toBe(false)
      await click(more)
    }
  }
  const unpair = nodes(root).find(node => String(node.props.class).includes('dropdown-item') && node.text === 'Unpair')!
  await click(unpair)
  expect(remove).toHaveBeenCalledOnce()
  expect(nodes(root).some(node => String(node.props.class).includes('dropdown-item'))).toBe(false)
})
