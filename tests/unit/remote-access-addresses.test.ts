import { beforeEach, expect, it, vi } from 'vitest'
import { createSSRApp, h, nextTick, type FunctionalComponent } from 'vue'
import { renderToString } from 'vue/server-renderer'
import RemoteAccessAddresses from '@/views/home/remote-access/RemoteAccessAddresses.vue'
import { click, clientTemplate, content, nodes, renderView } from '../helpers/vue-node-renderer'

const mock = vi.hoisted(() => ({ modal: vi.fn() }))
vi.mock('@/components/modal', () => ({ pushModal: mock.modal }))
beforeEach(() => vi.clearAllMocks())

const ButtonStub: FunctionalComponent<{ text?: string }> = (props, { slots }) => h('button', props.text ?? slots.default?.())
ButtonStub.props = { text: String }

const addressProps = {
  hostname: '',
  httpPort: 8280,
  httpsPort: 8643,
  httpAddresses: ['http://192.0.2.1:8280', 'http://192.0.2.2:8280', 'http://192.0.2.3:8280'],
  httpsAddresses: ['https://192.0.2.1:8643', 'https://192.0.2.2:8643', 'https://192.0.2.3:8643'],
  loadingIps: false,
  addressError: false,
}

async function addresses(enabled = true) {
  let state: any
  const component = RemoteAccessAddresses as any
  const app = createSSRApp(
    {
      ...component,
      setup(props: unknown, context: unknown) {
        state = component.setup(props, context)
        return state
      },
    },
    { ...addressProps, enabled }
  )
  app.config.globalProperties.$t = (key: string) => key
  app.directive('tooltip', {})
  app.component('VTextButton', ButtonStub)
  app.component('VIconButton', ButtonStub)
  app.component('VCopyButton', ButtonStub)
  const html = await renderToString(app)
  return { state, html }
}

it('switches protocol and keeps the selected host for displayed URLs and QR content', async () => {
  const { state, html } = await addresses()
  expect(html).toContain('http://192.0.2.1:8280')
  state.selectedHost.value = '192.0.2.2'
  state.protocol.value = 'https'
  expect(state.address.value).toBe('https://192.0.2.2:8643')
  expect(state.port.value).toBe(8643)
  state.showQr()
  const currentAddress = mock.modal.mock.calls[0][1].address
  expect(currentAddress()).toBe('https://192.0.2.2:8643')
  state.protocol.value = 'http'
  expect(currentAddress()).toBe('http://192.0.2.2:8280')
})

it.each(['http', 'https'] as const)('opens a QR for each alternate %s address without selecting it', async (protocol) => {
  const component = clientTemplate({ ...RemoteAccessAddresses }, 'src/views/home/remote-access/RemoteAccessAddresses.vue')
  const view = renderView(component, { ...addressProps, enabled: true }, {
    VTextButton: ButtonStub,
    VIconButton: ButtonStub,
    VCopyButton: ButtonStub,
  })
  try {
    click(nodes(view.root).find((node) => node.props.id === `remote-${protocol}-tab`)!)
    await nextTick()
    click(nodes(view.root).find((node) => node.props.class === 'other-addresses')!)
    await nextTick()
    const rows = () => nodes(view.root).filter((node) => node.props.class === 'address-row')
    const urls = protocol === 'http' ? addressProps.httpAddresses : addressProps.httpsAddresses
    for (const [index, url] of urls.slice(1).entries()) {
      const qr = nodes(rows()[index + 1]).find((node) => node.props['aria-label'] === 'remote_access_qr')
      expect(qr, url).toBeDefined()
      click(qr!)
      const modal = mock.modal.mock.calls[index][1]
      expect(modal.address()).toBe(url)
    }
    await nextTick()
    expect(rows()).toHaveLength(3)
    expect(content(rows()[0])).toContain(urls[0])
  } finally {
    view.unmount()
  }
})

it('explains disabled remote access while preserving port editing and both protocol tabs', async () => {
  const { html } = await addresses(false)
  expect(html).toContain('remote_access_off')
  expect(html).toContain('HTTPS')
  expect(html).toContain('8280')
  expect(html).not.toContain('http://192.0.2.1:8280')
  expect(mock.modal).not.toHaveBeenCalled()
})

it('shows the hostname inside other addresses with diagnostics, edit, copy and a live protocol QR', async () => {
  const component = clientTemplate({ ...RemoteAccessAddresses }, 'src/views/home/remote-access/RemoteAccessAddresses.vue')
  const diagnostics = vi.fn()
  const edit = vi.fn()
  const view = renderView(component, { ...addressProps, hostname: 'fixture.local', enabled: true, httpAddresses: [addressProps.httpAddresses[0]], onDiagnostics: diagnostics, onEditHostname: edit }, {
    VTextButton: ButtonStub, VIconButton: ButtonStub, VCopyButton: ButtonStub,
  })
  try {
    expect(content(view.root)).not.toContain('fixture.local')
    click(nodes(view.root).find(node => node.props.class === 'other-addresses')!)
    await nextTick()
    const row = nodes(view.root).find(node => node.props.class === 'address-row' && content(node).includes('fixture.local'))!
    expect(content(row)).toContain('fixture.local:8280')
    const buttons = nodes(row).filter(node => node.tag === 'button')
    expect(buttons.map(node => node.props['aria-label'] ?? content(node))).toEqual(['mdns_debug', 'edit: mdns_hostname', 'http://fixture.local:8280', 'remote_access_qr'])
    click(buttons[0]); click(buttons[1]); click(buttons[3])
    expect(diagnostics).toHaveBeenCalledOnce()
    expect(edit).toHaveBeenCalledOnce()
    const qr = mock.modal.mock.calls[0][1].address
    expect(qr()).toBe('http://fixture.local:8280')
    click(nodes(view.root).find(node => node.props.id === 'remote-https-tab')!)
    await nextTick()
    expect(qr()).toBe('https://fixture.local:8643')
  } finally { view.unmount() }
})
