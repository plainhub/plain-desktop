import { expect, it } from 'vitest'
import { nearbyGroups, nearbyPairAction } from '@/lib/device/nearby'
import type { IPeer } from '@/lib/interfaces'
import { DeviceType, PeerStatus } from '@/lib/status'
import type { DiscoveredDevice } from '@/hooks/use-device-discovery'

function peer(id: string, paired = true): IPeer {
  return { id, name: id, ip: '192.0.2.10', port: 8443, deviceType: DeviceType.PHONE, online: false, status: paired ? PeerStatus.PAIRED : PeerStatus.UNPAIRED, createdAt: '', updatedAt: '' }
}
function device(id: string, remoteAccess?: boolean): DiscoveredDevice {
  return { id, name: id, ips: ['192.0.2.10'], port: 8443, deviceType: DeviceType.PHONE, version: '', platform: '', lastSeen: '', status: PeerStatus.UNPAIRED, discoveryMethods: ['LAN'], remoteAccess }
}
it('keeps offline chat and management peers in Paired devices and deduplicates discoveries by identity', () => {
  const chat = peer('chat'),
    managed = peer('managed')
  const result = nearbyGroups([chat, managed, peer('new', false), peer('self')], [device('chat'), device('managed'), device('new'), device('self')], 'self')
  expect(result.mine).toEqual([chat, managed])
  expect(result.nearby.map((value) => value.id)).toEqual(['new'])
})
it('sends remote-off and unknown discoveries directly to chat and only offers both modes when explicitly enabled', () => {
  expect(nearbyPairAction(undefined, device('new', false))).toBe('chat')
  expect(nearbyPairAction(undefined, device('new'))).toBe('chat')
  expect(nearbyPairAction(undefined, device('new', true))).toBe('pair')
})
it('upgrades a chat relationship through full control and switches an existing management relationship', () => {
  expect(nearbyPairAction(peer('chat'), device('chat', true))).toBe('control')
  expect(nearbyPairAction(peer('chat'), device('chat', false))).toBe('control')
  expect(nearbyPairAction(peer('managed'), undefined, true)).toBe('switch')
})

it('keeps existing management credentials visible even without chat pairing', () => {
  const managed = peer('managed', false)
  const result = nearbyGroups([managed], [device('managed')], undefined, new Set(['managed']))
  expect(result.mine).toEqual([managed])
  expect(result.nearby).toEqual([])
})
