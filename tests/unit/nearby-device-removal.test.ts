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
import listItemSource from '@/components/base/VListItem.vue?raw'
import { unpairPeerGQL } from '@/lib/api/mutation'
import { peersGQL } from '@/lib/api/query'
import { DeviceType, PeerStatus } from '@/lib/status'
import common from '@/locales/en-US/common'
import messages from '@/locales/en-US'

const io = vi.hoisted(() => ({ request: vi.fn(), removePref: vi.fn(), clearRemote: vi.fn(), current: '' }))
vi.mock('@/lib/api/gql-client', () => ({ nativeGqlFetch: io.request }))
vi.mock('@/lib/api/subscription-client', () => ({
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
}))
vi.mock('@/lib/device/login-peers', async () => ({
  loginPeers: (await import('vue')).ref([]),
  findLoginPeer: (id: string) => id === io.current ? { id } : undefined,
  preloadLoginPeers: async () => {},
}))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => !io.current }))
vi.mock('@/lib/device/self-device', () => ({ loadSelfDevice: async () => null }))
vi.mock('@/lib/prefs/browser', () => ({ remove: io.removePref }))
vi.mock('@/lib/device/pairing', async () => {
  const { ref } = await import('vue')
  return { createPairing: () => ({ state: ref(null), dispose() {} }) }
})
vi.mock('@/hooks/use-device-discovery', async () => {
  const { ref } = await import('vue')
  return {
    useDeviceDiscovery: () => ({
      devices: ref([]), pairingStates: new Map(), start: async () => {}, stop: async () => {},
    }),
  }
})
vi.mock('@/components/DeviceDiscoveryStatus.vue', () => ({ default: () => null }))
vi.mock('@/components/MdnsFirewallFix.vue', () => ({ default: () => null }))
vi.mock('@/views/login/DiscoveredDeviceLogin.vue', () => ({ default: () => null }))

const peer = { id: 'remote', name: 'Test device', deviceType: DeviceType.PHONE, status: PeerStatus.PAIRED }

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

async function mount(current = '', fail = false) {
  io.current = current
  const location = { href: '/nearby' }
  vi.stubGlobal('window', { location })
  vi.stubGlobal('__IS_TAURI__', true)
  let paired = true
  io.request.mockImplementation(async (document: string) => {
    if (document === peersGQL) return { data: { peers: [{ ...peer, status: paired ? PeerStatus.PAIRED : PeerStatus.UNPAIRED }] } }
    if (document !== unpairPeerGQL) throw new Error('Unexpected GraphQL request')
    if (fail) return { errors: [{ message: 'Device unavailable' }] }
    paired = false
    return { data: { unpairPeer: true } }
  })
  const app = renderer.createApp(NearbyView)
  app.provide(ssrContextKey, { modules: new Set() })
  app.use(createI18n({ legacy: false, locale: 'en-US', messages: { 'en-US': messages } }))
  app.component('VListItem', VListItem)
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
  return { location, openRemoval, clickText, text: () => nodes(root).map((node) => node.text).join(' ') }
}

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
  expect(io.request.mock.calls.filter(([document]) => document === unpairPeerGQL)).toEqual([[unpairPeerGQL, { id: peer.id }]])
  expect(io.removePref).toHaveBeenCalledWith('main_state:remote')
  expect(io.clearRemote).toHaveBeenCalledTimes(current ? 1 : 0)
  expect(view.location.href).toBe(current ? '/' : '/nearby')
  if (!current) expect(view.text()).not.toContain('Test device')
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
