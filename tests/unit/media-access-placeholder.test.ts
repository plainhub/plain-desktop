import { createSSRApp } from 'vue'
import { renderToString } from '@vue/server-renderer'
import { createI18n } from 'vue-i18n'
import { expect, it, vi } from 'vitest'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import zhCN from '@/locales/zh-CN'

vi.mock('@/lib/api/mutation', () => ({ requestAccessGQL: '', initMutation: () => ({}) }))
vi.mock('@/lib/api/peer-client', () => ({ gqlFetchPeer: vi.fn() }))
vi.mock('@/lib/device/login-peers', () => ({ findLoginPeer: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/plugins/tapphone', () => ({ default: vi.fn() }))

it.each([
  ['NO_PERMISSION', '当前没有访问权限。请到手机应用 &gt; “访问设置”里打开相应开关。', true],
  ['NETWORK_ERROR', zhCN.NETWORK_ERROR, false],
])('renders %s in the list with its applicable action', async (key, message, canRequest) => {
  const app = createSSRApp(NoDataPlaceholder, { loading: false, feature: 'IMAGES', placeholderKey: key })
  app.use(createI18n({ legacy: false, locale: 'zh-CN', messages: { 'zh-CN': zhCN } }))
  app.component('v-text-button', { template: '<button><slot /></button>' })
  const html = await renderToString(app)
  expect(html).toContain(message)
  expect(html.includes('<button')).toBe(canRequest)
})
