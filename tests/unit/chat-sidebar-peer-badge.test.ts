import { expect, it, vi } from 'vitest'
import { createSSRApp, h, type Slots } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { createPinia, defineStore } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import DeviceTypeIcon from '@/components/DeviceTypeIcon.vue'
import PeerIcon from '@/components/PeerIcon.vue'
import SidebarListItem from '@/components/SidebarListItem.vue'
import SidebarPeerListItem from '@/views/chat/components/SidebarPeerListItem.vue'
import { DeviceType } from '@/lib/status'

vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('@/stores/chat', () => ({ useChatStore: () => ({}) }))
vi.mock('@/stores/temp', () => ({ useTempStore: defineStore('test-chat-sidebar-temp', { state: () => ({ urlTokenKey: '' }) }) }))
vi.mock('@/plugins/router', () => ({ replacePath: vi.fn() }))
vi.mock('@/lib/device/login-peers', () => ({ findLoginPeer: (id: string) => id === 'native-managed' ? { id } : undefined }))
vi.mock('@/lib/api/mutation', () => ({ deletePeerGQL: '', initMutation: () => ({ mutate: vi.fn() }) }))
vi.mock('@/views/chat/hooks/chat', () => ({ clearChatMessages: vi.fn(), useTasks: () => ({ cancelByChatId: vi.fn() }) }))

async function render(id: string, managed: boolean, online: boolean) {
  const app = createSSRApp(SidebarPeerListItem, { peer: { id, name: 'Fixture phone', managed, deviceType: DeviceType.PHONE }, online })
  app.use(createPinia())
  app.use(createRouter({ history: createMemoryHistory(), routes: [] }))
  app.config.globalProperties.$t = (key: string) => key
  app.directive('tooltip', { getSSRProps: (binding) => ({ 'data-tooltip': binding.value }) })
  app.component('SidebarListItem', SidebarListItem)
  app.component('PeerIcon', PeerIcon)
  app.component('DeviceTypeIcon', DeviceTypeIcon)
  app.component('VDropdown', (_: unknown, { slots }: { slots: Slots }) => h('span', slots.trigger?.()))
  app.component('VIconButton', () => null)
  app.component('DeleteConfirmPopup', () => null)
  return renderToString(app)
}

it('renders the same managed key and tooltip on an offline row without an online dot', async () => {
  const offline = await render('web-peer', true, false)
  const online = await render('web-peer', true, true)
  for (const row of [offline, online]) {
    expect(row).toContain('class="login-badge"')
    expect(row).toContain('data-tooltip="login_device_hint"')
    expect(row).toContain('key-rounded')
  }
  expect(offline).not.toContain('class="online-dot"')
  expect(online).toContain('class="online-dot"')
})

it('does not mark an unmanaged offline row and preserves the native signed-in badge', async () => {
  const unmanaged = await render('web-peer', false, false)
  expect(unmanaged).not.toContain('class="login-badge"')
  expect(unmanaged).not.toContain('class="online-dot"')
  const native = await render('native-managed', false, false)
  expect(native).toContain('class="login-badge"')
  expect(native).not.toContain('class="online-dot"')
})
