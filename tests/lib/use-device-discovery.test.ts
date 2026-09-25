import { describe, expect, it, vi } from 'vitest'
import emitter from '@/plugins/eventbus'
import { useDeviceDiscovery } from '@/hooks/use-device-discovery'

vi.mock('@/lib/api/mutation', () => ({
  startDiscoveryGQL: 'startDiscovery',
  stopDiscoveryGQL: 'stopDiscovery',
  initMutation: ({ document }: { document: string }) => ({
    mutate: async () => {
      ;(globalThis as any).__discoveryCalls.push(document)
    },
  }),
}))

vi.mock('@/lib/api/gql-client', () => ({
  gqlFetchOp: async () => ({ data: { isDiscovering: true } }),
}))

vi.mock('@/lib/api/query', () => ({ isDiscoveringGQL: 'isDiscovering' }))
vi.mock('@/stores/chat', () => ({ useChatStore: () => ({ fetchPeers: async () => {} }) }))

describe('useDeviceDiscovery', () => {
  it('shows saved devices, removes unreachable devices, and requests saved devices after reconnect', async () => {
    ;(globalThis as any).__discoveryCalls = []
    const discovery = useDeviceDiscovery()
    await discovery.start()
    const device = {
      id: 'phone-1', name: 'Phone', ips: ['192.168.1.2'], port: 8443,
      deviceType: 'PHONE', version: '1', platform: 'android',
      lastSeen: '2026-09-25T00:00:00Z', status: 'UNPAIRED', discoveryMethods: ['LAN'],
    }
    emitter.emit('nearby_device_found', device)
    expect(discovery.devices.value.map((item) => item.id)).toEqual(['phone-1'])
    emitter.emit('nearby_device_unreachable', { id: 'phone-1' })
    expect(discovery.devices.value).toEqual([])
    emitter.emit('app_socket_connection_changed', true)
    await Promise.resolve()
    expect((globalThis as any).__discoveryCalls).toEqual(['startDiscovery', 'startDiscovery'])
    await discovery.stop()
  })
})
