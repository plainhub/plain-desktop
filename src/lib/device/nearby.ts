import type { DiscoveredDevice } from '@/hooks/use-device-discovery'
import type { IPeer } from '@/lib/interfaces'
import { PeerStatus } from '@/lib/status'
import { buildUrl } from '@/lib/url'

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

export function webSwitchDeviceUrl(device: Pick<DiscoveredDevice, 'ips' | 'bestIp' | 'port' | 'remoteAccess'>): string | undefined {
  // Discovery advertises an HTTPS port. Never pass the current origin's identity or credentials.
  if (device.remoteAccess !== true || !Number.isInteger(device.port) || device.port < 1 || device.port > 65535) return
  for (const ip of [device.bestIp, ...device.ips]) {
    if (!ip || !(/^(?:\d{1,3}\.){3}\d{1,3}$/.test(ip) || (ip.includes(':') && /^[\da-f:.]+$/i.test(ip)))) continue
    try {
      return new URL(buildUrl('https', ip.includes(':') ? `[${ip}]` : ip, device.port, '/')).href
    } catch { /* Try the next advertised IP. */ }
  }
}
