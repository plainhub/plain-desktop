import { afterEach, expect, it, vi } from 'vitest'
import * as Vue from 'vue'
import { createRenderer, defineComponent, h, nextTick, reactive, ssrContextKey } from 'vue'
import { compileScript, compileTemplate, parse } from 'vue/compiler-sfc'
import TauriTabBar from '@/components/TauriTabBar.vue'
import source from '@/components/TauriTabBar.vue?raw'
import { setRemoteClientId } from '@/lib/device/client-id'

const fixture = vi.hoisted(() => ({
  store: { app: { deviceName: '' } },
  native: vi.fn(),
  current: vi.fn(),
  addresses: vi.fn(),
}))

vi.mock('vue-router', () => ({ useRouter: () => ({}), useRoute: () => ({ meta: {} }) }))
vi.mock('pinia', async () => ({ storeToRefs: (await import('vue')).toRefs }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => fixture.store }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({ tabs: [{ id: 'home' }], activeTabId: 'home' }) }))
vi.mock('@/lib/device/login-peers', async () => {
  const peers = [{ id: 'remote', name: 'Remote device' }]
  return { loginPeers: (await import('vue')).ref(peers), findLoginPeer: (id: string) => peers.find(peer => peer.id === id) }
})
vi.mock('@/lib/api/gql-client', () => ({ nativeGqlFetch: fixture.native, gqlFetch: fixture.current }))
vi.mock('@/lib/api/local-server', () => ({ readLocalAddresses: fixture.addresses }))
vi.mock('@/plugins/i18n', () => ({ default: {} }))
vi.mock('@/components/modal', () => ({ pushModal: vi.fn() }))
vi.mock('@/components/contextmenu', () => ({ contextmenu: vi.fn() }))
vi.mock('@/views/chat/NearbyView.vue', () => ({ default: {} }))

const { descriptor } = parse(source)
const template = compileTemplate({
  source: descriptor.template!.content,
  filename: 'TauriTabBar.vue',
  id: 'tab-bar',
  compilerOptions: {
    mode: 'function',
    prefixIdentifiers: true,
    bindingMetadata: compileScript(descriptor, { id: 'tab-bar' }).bindings,
    isCustomElement: tag => tag.startsWith('i-material-symbols:'),
  },
})
TauriTabBar.render = new Function('Vue', template.code)(Vue)

type Node = { text: string; children: Node[]; parent?: Node; onClick?: () => void }
const node = (text = ''): Node => ({ text, children: [] })
function remove(child: Node) {
  const siblings = child.parent?.children
  if (siblings) siblings.splice(siblings.indexOf(child), 1)
  child.parent = undefined
}

const renderer = createRenderer<Node, Node>({
  insert(child, parent, anchor) {
    remove(child)
    child.parent = parent
    parent.children.splice(anchor ? parent.children.indexOf(anchor) : parent.children.length, 0, child)
  },
  remove,
  createElement: () => node(),
  createText: node,
  createComment: () => node(),
  setText: (target, text) => { target.text = text },
  setElementText: (target, text) => { target.text = text },
  parentNode: child => child.parent ?? null,
  nextSibling: child => child.parent?.children[child.parent.children.indexOf(child) + 1] ?? null,
  patchProp: (target, key, _, value) => { if (key === 'onClick') target.onClick = value },
})

function descendants(root: Node): Node[] {
  return [root, ...root.children.flatMap(descendants)]
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
  vi.resetAllMocks()
})

it.each(['remote', ''])('shows the local name with selected peer "%s"', async (peerId) => {
  vi.stubGlobal('__IS_TAURI__', true)
  vi.stubGlobal('window', new EventTarget())
  vi.spyOn(console, 'error').mockImplementation(() => {})
  setRemoteClientId(peerId)
  fixture.store = reactive({ app: { deviceName: peerId ? 'Remote device' : 'Local workstation' } })
  fixture.native.mockResolvedValue({ data: { app: { deviceName: 'Local workstation' } } })
  fixture.current.mockResolvedValue({ data: { app: { deviceName: 'Local workstation' } } })
  fixture.addresses.mockRejectedValue(new Error('Addresses unavailable'))

  const app = renderer.createApp(TauriTabBar)
  app.provide(ssrContextKey, { modules: new Set() })
  app.config.globalProperties.$t = (key: string) => key
  app.component('VDropdown', defineComponent({
    inheritAttrs: false,
    props: { modelValue: Boolean },
    emits: ['update:modelValue'],
    setup(props, { emit, slots }) {
      return () => [
        h('button', { onClick: () => emit('update:modelValue', !props.modelValue) }, 'Toggle device menu'),
        props.modelValue ? slots.default?.() : slots.trigger?.(),
      ]
    },
  }))

  const root = node()
  app.mount(root)
  try {
    const toggleMenu = () => descendants(root).find(item => item.text === 'Toggle device menu')!.onClick!()
    const localRow = () => descendants(root).find(item => item.children.some(child => child.text === 'device_discovery.local'))
    toggleMenu()
    await new Promise<void>(resolve => setImmediate(resolve))

    expect(localRow()?.children.map(child => child.text)).toEqual(['Local workstation', 'device_discovery.local'])
    expect(descendants(root).some(item => item.text === 'Remote device')).toBe(true)
    expect(fixture.native).toHaveBeenCalledTimes(peerId ? 1 : 0)
    expect(fixture.addresses).not.toHaveBeenCalled()
    expect(fixture.current).not.toHaveBeenCalled()

    if (peerId) {
      toggleMenu()
      await nextTick()
      fixture.native.mockResolvedValue({ data: { app: { deviceName: 'Renamed workstation' } } })
      toggleMenu()
      await new Promise<void>(resolve => setImmediate(resolve))
    } else {
      fixture.store.app.deviceName = 'Renamed workstation'
      await nextTick()
    }
    expect(localRow()?.children[0]?.text).toBe('Renamed workstation')
  } finally {
    app.unmount()
  }
})
