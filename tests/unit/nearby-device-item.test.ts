import { expect, it } from 'vitest'
import { createSSRApp, h, type Slots } from 'vue'
import { renderToString } from 'vue/server-renderer'
import NearbyDeviceItem from '@/views/chat/NearbyDeviceItem.vue'
import PeerIcon from '@/components/PeerIcon.vue'
import DeviceTypeIcon from '@/components/DeviceTypeIcon.vue'
import VListItem from '@/components/base/VListItem.vue'
import VOutlinedButton from '@/components/base/VOutlinedButton.vue'
import { DeviceType, PeerStatus } from '@/lib/status'

const device = {
  id: 'fixture',
  name: 'Fixture phone',
  ips: ['192.0.2.10'],
  bestIp: '192.0.2.10',
  port: 8443,
  deviceType: DeviceType.PHONE,
  version: '',
  platform: '',
  lastSeen: '',
  status: PeerStatus.UNPAIRED,
  discoveryMethods: ['LAN'],
}
const peer = { id: 'fixture', name: 'Fixture phone', ip: '192.0.2.10', port: 8443, deviceType: DeviceType.PHONE, status: PeerStatus.PAIRED, online: true, createdAt: '', updatedAt: '' }
const wrapper = (_: unknown, { slots }: { slots: Slots }) => h('span', [slots.trigger?.(), slots.default?.()])
async function render(props: Record<string, unknown>) {
  const app = createSSRApp(NearbyDeviceItem, props)
  app.config.globalProperties.$t = (key: string) => key
  app.directive('tooltip', {})
  for (const [name, component] of Object.entries({
    VListItem,
    VOutlinedButton,
    PeerIcon,
    DeviceTypeIcon,
    VDropdown: wrapper,
    VIconButton: wrapper,
    VDropdownMenu: () => null,
    VCircularProgress: wrapper,
  }))
    app.component(name, component)
  return renderToString(app)
}
it('renders the real outlined chat action for remote-off discoveries and shows their IP', async () => {
  const html = await render({ device: { ...device, remoteAccess: false } })
  expect(html).toContain('pairing_chat_only')
  expect(html).not.toContain('pairing_full_control')
  expect(html).toContain('192.0.2.10')
  expect(html).toContain('v-outlined-button')
})
it('renders an upgrade action, keeps it disabled when remote access is off, and uses the shared online icon', async () => {
  const html = await render({ peer, device: { ...device, remoteAccess: false } })
  expect(html).toContain('pairing_full_control')
  expect(html).toMatch(/<button[^>]*disabled/)
  expect(html).toContain('online-dot')
  expect(html).not.toContain('pairing_chat_only')
})
it('enables the upgrade when remote access is on, without an online text label', async () => {
  const html = await render({ peer, device: { ...device, remoteAccess: true } })
  expect(html).not.toMatch(/<button[^>]*disabled/)
  expect(html).not.toContain('>online<')
})
it('keeps offline peers visible without a green dot and disables their upgrade', async () => {
  const html = await render({ peer: { ...peer, online: false } })
  expect(html).toContain('Fixture phone')
  expect(html).not.toContain('class="online-dot"')
  expect(html).toMatch(/<button[^>]*disabled/)
})

it('switches an authenticated peer instead of offering another pairing or upgrade', async () => {
  const html = await render({ peer, fullControl: true, device: { ...device, remoteAccess: true } })
  expect(html).toContain('switch')
  expect(html).not.toContain('pairing_full_control')
  expect(html).not.toMatch(/<button[^>]*disabled/)
})
