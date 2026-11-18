import type { DiscoveredDevice } from '@/hooks/use-device-discovery'
import type { IPeer } from '@/lib/interfaces'
import { PeerStatus } from '@/lib/status'

function compareDevices(left: { id: string; name: string }, right: { id: string; name: string }) {
  return left.name.localeCompare(right.name, undefined, { sensitivity: 'base' }) || left.id.localeCompare(right.id)
}

export function nearbyGroups(peers: IPeer[], devices: DiscoveredDevice[], selfId?: string, managedIds?: ReadonlySet<string>) {
  const mine = peers.filter((peer) => peer.id !== selfId && (peer.status === PeerStatus.PAIRED || managedIds?.has(peer.id))).sort(compareDevices)
  const paired = new Set(mine.map((peer) => peer.id))
  const nearby = devices.filter((device) => device.id !== selfId && !paired.has(device.id)).sort(compareDevices)
  return { mine, nearby }
}

export function nearbyPairAction(peer: IPeer | undefined, device: DiscoveredDevice | undefined, fullControl = false) {
  if (fullControl) return 'switch'
  if (peer?.status === PeerStatus.PAIRED) return 'control'
  return device?.remoteAccess === true ? 'pair' : 'chat'
}
