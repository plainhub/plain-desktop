import { createApp, h, ref } from 'vue'
import { createPinia } from 'pinia'
import NearbyDeviceItem from '@/views/chat/NearbyDeviceItem.vue'
import NearbyPairOptions from '@/views/chat/NearbyPairOptions.vue'
import tooltip from '@/plugins/tooltip'
import { DeviceType, PeerStatus } from '@/lib/status'
import '@/styles/main.scss'

const labels: Record<string, string> = {
  switch: '切换', pair: '配对', pairing_chat_only: '仅聊天', pairing_full_control: '完全控制',
  pairing_access_prompt: '选择与这台设备的连接方式', pairing_chat_only_hint: '互发消息和文件',
  pairing_full_control_hint: '登录管理设备，并互发消息与文件', pairing_start: '继续',
  cancel: '取消', actions: '更多操作', unpair: '解除配对',
  'device_discovery.current': '当前设备', login_device_hint: '登录设备 · 完全控制',
}
const device = (id: string, remoteAccess: boolean) => ({ id, name: id, ips: ['192.0.2.10'], bestIp: '192.0.2.10', port: 8443,
  deviceType: DeviceType.PHONE, version: '', platform: '', lastSeen: '', status: PeerStatus.UNPAIRED, discoveryMethods: ['LAN'] as ('LAN' | 'BLE')[], remoteAccess })
const peer = { id: '已配对手机', name: '已配对手机', ip: '192.0.2.20', port: 8443, deviceType: DeviceType.PHONE,
  status: PeerStatus.PAIRED, online: true, createdAt: '', updatedAt: '' }

const app = createApp({ setup() {
  const selected = ref(false)
  const event = ref('')
  return () => h('div', { style: 'max-width:560px;margin:24px auto;display:flex;flex-direction:column;gap:24px;' }, [
    h('h2', '附近设备'),
    selected.value ? h(NearbyPairOptions, { onCancel: () => { selected.value = false }, onConfirm: (mode: string) => { selected.value = false; event.value = mode } }) : [
      h('h3', '已配对设备'),
      h('ul', { class: 'card list-items' }, [
        h(NearbyDeviceItem, { peer, device: device(peer.id, true), onConnect: (action: string) => { event.value = action }, onRemove: () => { event.value = '解除配对' } }),
        h(NearbyDeviceItem, { peer: { ...peer, id: '未开启远程访问', name: '未开启远程访问' }, device: device('未开启远程访问', false) }),
      ]),
      h('h3', '正在搜索附近的 PlainApp 设备…'),
      h('ul', { class: 'card list-items' }, [
        h(NearbyDeviceItem, { device: device('聊天设备', false), onConnect: (action: string) => { event.value = action } }),
        h(NearbyDeviceItem, { device: device('可管理设备', true), onConnect: () => { selected.value = true } }),
      ]),
    ],
    h('output', { 'aria-live': 'polite' }, event.value),
  ])
} })
app.config.globalProperties.$t = (key: string) => labels[key] ?? key
app.use(createPinia()).use(tooltip).mount('#app')
