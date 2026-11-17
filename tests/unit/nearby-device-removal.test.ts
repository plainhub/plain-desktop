import { afterEach, expect, it, vi } from 'vitest'
import { createRenderer, h, nextTick, ssrContextKey } from 'vue'
import * as Vue from 'vue'
import { compileScript, compileTemplate, parse } from 'vue/compiler-sfc'
import { createI18n } from 'vue-i18n'
import { transpile } from 'typescript'
import NearbyView from '@/views/chat/NearbyView.vue'
import nearbyViewSource from '@/views/chat/NearbyView.vue?raw'
import NearbyDeviceItem from '@/views/chat/NearbyDeviceItem.vue'
import nearbyDeviceItemSource from '@/views/chat/NearbyDeviceItem.vue?raw'
import VListItem from '@/components/base/VListItem.vue'
import InlineDeleteConfirm from '@/components/base/InlineDeleteConfirm.vue'
import inlineSource from '@/components/base/InlineDeleteConfirm.vue?raw'
import listItemSource from '@/components/base/VListItem.vue?raw'
import { unpairPeerGQL } from '@/lib/api/mutation'
import { peersGQL } from '@/lib/api/query'
import { DeviceType, PeerStatus } from '@/lib/status'
import type { DiscoveredDevice } from '@/hooks/use-device-discovery'
import type { SelfDevice } from '@/lib/device/self-device'
import common from '@/locales/en-US/common'
import messages from '@/locales/en-US'

const io = vi.hoisted(() => ({ request: vi.fn(), probe: vi.fn(), pushModal: vi.fn(), removePref: vi.fn(), clearRemote: vi.fn(), setRemote: vi.fn(), start: vi.fn(), current: '', devices: [] as DiscoveredDevice[], self: null as SelfDevice | null }))
vi.mock('@/components/modal', () => ({ popModal: vi.fn(), pushModal: io.pushModal }))
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
  return { createPairing: () => ({ state: ref(null), dispose() {} }) }
})
vi.mock('@/hooks/use-device-discovery', async () => {
  const { ref } = await import('vue')
  return {
    useDeviceDiscovery: () => ({
      devices: ref(io.devices), pairingViews: new Map(), start: io.start, stop: async () => {},
    }),
  }
})
vi.mock('@/components/DeviceDiscoveryStatus.vue', () => ({ default: () => h('p', 'Looking for nearby devices') }))
vi.mock('@/components/MdnsFirewallFix.vue', () => ({ default: () => null }))
vi.mock('@/views/login/DiscoveredDeviceLogin.vue', () => ({ default: () => null }))

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
      isCustomElement: (tag) => tag.startsWith('i-material-symbols:'),
    },
  })
  if (errors.length) throw new Error(errors.join('\n'))
  return new Function('Vue', transpile(code))(Vue)
}
NearbyView.render = clientRender(nearbyViewSource)
NearbyDeviceItem.render = clientRender(nearbyDeviceItemSource)
VListItem.render = clientRender(listItemSource)
InlineDeleteConfirm.render = clientRender(inlineSource)

type Node = { text: string; children: Node[]; parent: Node | null; props: Record<string, unknown> }

const makeNode = (text = ''): Node => ({ text, children: [], parent: null, props: {} })
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
  insert(node, parent, anchor) {
    removeNode(node)
    node.parent = parent
    parent.children.splice(anchor ? parent.children.indexOf(anchor) : parent.children.length, 0, node)
  },
  remove: removeNode,
})

function nodes(node: Node): Node[] {
  return [node, ...node.children.flatMap(nodes)]
}

let unmount: (() => void) | undefined
afterEach(async () => {
  unmount?.()
  await Promise.resolve()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
  vi.resetAllMocks()
})

async function mount(current = '', fail = false, fixtures: { paired?: boolean; devices?: DiscoveredDevice[]; self?: SelfDevice; switchOnly?: boolean; online?: boolean } = {}) {
  io.current = current
  io.devices = fixtures.devices ?? []
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
  const app = renderer.createApp(NearbyView, { switchOnly: fixtures.switchOnly })
  app.provide(ssrContextKey, { modules: new Set() })
  app.use(createI18n({ legacy: false, locale: 'en-US', messages: { 'en-US': messages } }))
  app.component('VListItem', VListItem)
  app.component('InlineDeleteConfirm', InlineDeleteConfirm)
  app.component('VModal', (_, { slots }) => h('section', [slots.headline?.(), slots.content?.(), slots.actions?.()]))
  app.component('VDropdownMenu', (props, { slots }) => props.modelValue ? slots.default?.() : null)
  app.component('VDropdown', (_, { slots }) => h('div', slots.trigger?.()))
  for (const name of ['VIconButton', 'VOutlinedButton', 'VFilledButton', 'VTextButton']) {
    app.component(name, { setup: (_, { slots }) => () => h('button', slots.default?.()) })
  }
  app.component('PeerIcon', () => null)
  const root = makeNode()
  app.mount(root)
  unmount = () => app.unmount()
  await new Promise<void>((resolve) => setImmediate(resolve))
  const click = async (match: (node: Node) => boolean) => {
    const button = nodes(root).find((node) => typeof node.props.onClick === 'function' && match(node))
    expect(button).toBeDefined()
    expect(button?.props.disabled).not.toBe(true)
    const onClick = button!.props.onClick as (event: unknown) => void
    onClick({ stopPropagation() {} })
    await nextTick()
  }
  const clickText = (label: string) => click((node) => node.children.some((child) => child.text === label) || node.text === label)
  const openRemoval = async () => {
    await click((node) => node.props.id === 'nearby-actions-remote')
    expect(nodes(root).filter((node) => node.props.class === 'dropdown-item').map((node) => node.text)).toEqual(['Unpair'])
    await clickText('Unpair')
  }
  return { location, openRemoval, clickText, deviceLists: () => nodes(root).filter((node) => node.props.class === 'card list-items').length, text: () => nodes(root).map((node) => node.text).join(' ') }
}

it.each([undefined, self])('does not show the paired heading with no paired peer (self: %j)', async (local) => {
  const view = await mount('', false, { paired: false, devices: [discovered], self: local })
  expect(view.text()).not.toContain('Paired devices')
  expect(view.text()).toContain('Looking for nearby devices')
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
  expect(view.text()).toContain(common.network_error)
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
  reject(new GqlError('network_error', 502))
  await vi.waitFor(() => expect(io.pushModal).toHaveBeenCalledOnce())
  expect(io.pushModal.mock.calls[0]?.[1]).toEqual({ peerName: peer.name, originName: self.name })
  expect(view.text()).not.toContain('Switching…')
  expect(io.setRemote).not.toHaveBeenCalled()
  expect(view.location.reload).not.toHaveBeenCalled()
})

it('exposes Unpair for a stale managed row in the Web picker without requiring a connection', async () => {
  const view = await mount('', false, { switchOnly: true, online: false, self })
  await view.clickText('Unpair')
  expect(view.text()).toContain('Unpair “Test device”?')
  expect(io.request.mock.calls.every(([document]) => document === peersGQL)).toBe(true)
  expect(io.probe).not.toHaveBeenCalled()
})

it('confirms and removes a stale saved Web peer through the gateway using the declared peerId variable', async () => {
  const view = await mount('', false, { switchOnly: true, online: false, self })
  await view.clickText('Unpair')
  expect(view.text()).toContain('Your chat history is kept.')
  await view.clickText('Cancel')
  expect(io.request.mock.calls.every(([document]) => document === peersGQL)).toBe(true)
  await view.clickText('Unpair')
  await view.clickText('Confirm')
  await new Promise<void>(done => setImmediate(done))
  expect(io.request.mock.calls.filter(([document]) => document === unpairPeerGQL)).toEqual([[unpairPeerGQL, { peerId: peer.id }]])
  expect(view.text()).not.toContain(peer.name)
  expect(io.probe).not.toHaveBeenCalled()
})
