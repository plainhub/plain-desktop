import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  pairDevice: vi.fn(),
  start: vi.fn(),
  stop: vi.fn(),
}))

vi.mock('@/hooks/use-device-discovery', async () => {
  const { ref } = await import('vue')
  return {
    DiscoveryStatus: { SEARCHING: 1 },
    upsertDiscoveredDevice: vi.fn(),
    useDeviceDiscovery: () => ({
      devices: ref([{
        id: 'phone-1', name: 'Pixel', ips: ['192.0.2.5'], port: 8443,
        deviceType: 'PHONE', version: '', platform: 'android', lastSeen: '',
        discoveryMethods: ['LAN'], status: 'UNPAIRED',
      }]),
      status: ref(1),
      retry: vi.fn(), start: mocks.start, stop: mocks.stop,
    }),
  }
})

vi.mock('@/hooks/use-device-pairing', async () => {
  const { reactive } = await import('vue')
  return {
    DeviceState: { PAIRING: 1, CANCELING: 2, UNPAIRING: 3, UNPAIRED: 4 },
    useDevicePairing: () => ({
      deviceStates: reactive(new Map()),
      pairDevice: mocks.pairDevice,
      cancelPairing: vi.fn(),
    }),
  }
})

vi.mock('@/lib/api/mutation', () => ({
  unpairPeerGQL: '',
  initMutation: () => ({ mutate: vi.fn() }),
}))

vi.mock('@/stores/chat', () => ({ useChatStore: () => ({ fetchPeers: vi.fn() }) }))

vi.mock('@/views/login/DiscoveredDeviceLogin.vue', async () => {
  const { defineComponent, h } = await import('vue')
  return {
    default: defineComponent({
      name: 'DiscoveredDeviceLogin',
      props: ['device'],
      setup: () => () => h('div', { class: 'login-form' }),
    }),
  }
})

import NearbyModal from '@/views/chat/NearbyModal.vue'

const Modal = defineComponent({
  setup(_, { slots }) {
    return () => h('div', [slots.headline?.(), slots.content?.(), slots.actions?.()])
  },
})

const ListItem = defineComponent({
  setup(_, { slots }) {
    return () => h('li', [slots.title?.(), slots.end?.()])
  },
})

const Button = defineComponent({
  setup(_, { emit, slots }) {
    return () => h('button', { onClick: (event: MouseEvent) => emit('click', event) }, slots.default?.())
  },
})

function mountNearby() {
  return mount(NearbyModal, {
    global: {
      stubs: {
        VModal: Modal,
        VListItem: ListItem,
        VOutlinedButton: Button,
        VFilledButton: Button,
        VDropdown: defineComponent({ setup(_, { slots }) { return () => h('div', slots.trigger?.()) } }),
        DeviceTypeIcon: true,
        VCircularProgress: true,
        'i-lucide:bluetooth': true,
        'i-lucide:wifi': true,
        MdnsFirewallFix: true,
        QrPairPanel: true,
      },
      mocks: { $t: (key: string, vars?: { name?: string }) => vars?.name ? `${key} ${vars.name}` : key },
      directives: { tooltip: {} },
    },
  })
}

beforeEach(() => {
  mocks.pairDevice.mockReset().mockResolvedValue(true)
  mocks.start.mockReset()
  mocks.stop.mockReset()
})

describe('NearbyModal pairing choice', () => {
  it('opens the existing login flow for full control without sending a chat pair request', async () => {
    const wrapper = mountNearby()
    await wrapper.findAll('button').find((button) => button.text() === 'pair')?.trigger('click')
    await wrapper.findAll('button').find((button) => button.text() === 'pairing_start')?.trigger('click')
    expect(wrapper.find('.login-form').exists()).toBe(true)
    expect(mocks.pairDevice).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('sends the existing pairDevice mutation for chat only', async () => {
    const wrapper = mountNearby()
    await wrapper.findAll('button').find((button) => button.text() === 'pair')?.trigger('click')
    await wrapper.find('input[value="chat"]').setValue()
    await wrapper.findAll('button').find((button) => button.text() === 'pairing_start')?.trigger('click')
    await flushPromises()
    expect(mocks.pairDevice).toHaveBeenCalledWith(expect.objectContaining({ id: 'phone-1' }))
    expect(wrapper.find('.login-form').exists()).toBe(false)
    wrapper.unmount()
  })
})
