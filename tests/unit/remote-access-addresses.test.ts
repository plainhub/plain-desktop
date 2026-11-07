import { beforeEach, expect, it, vi } from 'vitest'
import { createSSRApp, h, type FunctionalComponent } from 'vue'
import { renderToString } from 'vue/server-renderer'
import RemoteAccessAddresses from '@/views/home/remote-access/RemoteAccessAddresses.vue'

const mock = vi.hoisted(() => ({ modal: vi.fn() }))
vi.mock('@/components/modal', () => ({ pushModal: mock.modal }))
beforeEach(() => vi.clearAllMocks())

const ButtonStub: FunctionalComponent<{ text?: string }> = (props, { slots }) => h('button', props.text ?? slots.default?.())
ButtonStub.props = { text: String }

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
    {
      httpPort: 8280,
      httpsPort: 8643,
      httpAddresses: ['http://192.0.2.1:8280', 'http://192.0.2.2:8280'],
      httpsAddresses: ['https://192.0.2.1:8643', 'https://192.0.2.2:8643'],
      enabled,
      loadingIps: false,
      addressError: false,
    }
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

it('explains disabled remote access while preserving port editing and both protocol tabs', async () => {
  const { html } = await addresses(false)
  expect(html).toContain('remote_access_off')
  expect(html).toContain('HTTPS')
  expect(html).toContain('8280')
  expect(html).not.toContain('http://192.0.2.1:8280')
  expect(mock.modal).not.toHaveBeenCalled()
})
