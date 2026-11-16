import type { DiscoveredDevice } from '@/hooks/use-device-discovery'
import type { IPeer } from '@/lib/interfaces'
import { PeerStatus } from '@/lib/status'

export function nearbyGroups(peers: IPeer[], devices: DiscoveredDevice[], selfId?: string, managedIds?: ReadonlySet<string>) {
  const mine = peers.filter((peer) => peer.id !== selfId && (peer.status === PeerStatus.PAIRED || managedIds?.has(peer.id))).sort((a, b) => a.name.localeCompare(b.name))
  const paired = new Set(mine.map((peer) => peer.id))
  return { mine, nearby: devices.filter((device) => device.id !== selfId && !paired.has(device.id)) }
}

export function nearbyPairAction(peer: IPeer | undefined, device: DiscoveredDevice | undefined, fullControl = false) {
  if (fullControl) return 'switch'
  if (peer?.status === PeerStatus.PAIRED) return 'control'
  return device?.remoteAccess === true ? 'pair' : 'chat'
}
