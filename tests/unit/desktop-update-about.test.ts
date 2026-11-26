import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import AboutView from '@/views/about/AboutView.vue'
import { click, clientTemplate, content, flush, nodes, renderView } from '../helpers/vue-node-renderer'

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), openUrl: vi.fn() }))
vi.mock('vue', async importOriginal => {
  const vue = await importOriginal<typeof import('vue')>()
  return { ...vue, Transition: vue.BaseTransition }
})
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({}) }) }))
vi.mock('@/lib/feature', () => ({ hasFeature: () => true }))
vi.mock('@/lib/api/gql-client', () => ({
  gqlFetchOp: async () => ({
    data: { appUpdate: { hasUpdate: true, latestVersion: '4.0.1', url: 'https://example.test/release' } },
  }),
}))
vi.mock('@/lib/api/native-invoke', () => ({ invoke: mocks.invoke }))
vi.mock('@/lib/browser', () => ({ openUrl: mocks.openUrl }))
vi.mock('@/lib/clipboard', () => ({ copyTextToClipboard: vi.fn() }))

clientTemplate(AboutView, 'src/views/about/AboutView.vue')
const Button = { template: '<button><slot /></button>' }

beforeEach(() => {
  vi.resetAllMocks()
  vi.stubGlobal('__APP_VERSION__', '4.0.0')
  vi.stubGlobal('navigator', { platform: 'Fixture', language: 'en-US', userAgent: 'Fixture' })
})
afterEach(() => vi.unstubAllGlobals())

async function about(supported: boolean) {
  vi.stubGlobal('window', { __PLAIN_DESKTOP_UPDATE__: supported })
  const view = renderView(AboutView, {}, { VFilledButton: Button })
  await flush()
  return { ...view, button: (label: string) => nodes(view.root).find(node => node.tag === 'button' && content(node) === label)! }
}

describe('About desktop updates', () => {
  it('runs the local update once without passing a URL or package path, and permits retry after failure', async () => {
    let fail!: (reason: string) => void
    mocks.invoke.mockImplementationOnce(() => new Promise((_resolve, reject) => { fail = reject }))
    const view = await about(true)
    try {
      click(view.button('install_update'))
      await flush()
      expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith('install_desktop_update')
      expect(view.button('installing_update').props.disabled).toBe(true)
      expect(mocks.openUrl).not.toHaveBeenCalled()
      fail('Update package checksum mismatch')
      await flush()
      expect(content(view.root)).toContain('Update package checksum mismatch')
      expect(view.button('install_update').props.disabled).toBe(false)
    } finally { view.unmount() }
  })
  it('does not offer the local command for store or system-managed installs', async () => {
    const view = await about(false)
    try {
      expect(view.button('install_update')).toBeUndefined()
      click(view.button('view_release'))
      expect(mocks.openUrl).toHaveBeenCalledExactlyOnceWith('https://example.test/release')
      expect(mocks.invoke).not.toHaveBeenCalled()
    } finally { view.unmount() }
  })
})
