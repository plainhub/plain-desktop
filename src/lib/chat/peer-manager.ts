import { initMutation, deletePeerGQL, unpairPeerGQL } from '@/lib/api/mutation'
import { peerCacher } from './peer-cacher'
import { chatCacher } from './chat-cacher'
import type { IPeer } from '@/lib/interfaces'
import { DeviceType, PeerStatus } from '@/lib/status'

class PeerManager {
  private readonly deleteMut = initMutation({ document: deletePeerGQL }, false)
  private readonly unpairMut = initMutation({ document: unpairPeerGQL }, false)

  async deletePeer(peerId: string): Promise<boolean> {
    const r = await this.deleteMut.mutate({ peerId })
    if (!r) return false
    peerCacher.removePeer(peerId)
    await peerCacher.load()
    await chatCacher.load()
    return true
  }

  async markUnpaired(peerId: string): Promise<boolean> {
    const r = await this.unpairMut.mutate({ peerId })
    if (!r) return false
    await peerCacher.load()
    return true
  }

  applyDeviceDiscovered(
    deviceId: string,
    ips: string[],
    port: number,
    name: string,
    deviceType: DeviceType,
  ): IPeer | null {
    const existing = peerCacher.getPeer(deviceId)
    if (!existing || existing.status !== PeerStatus.PAIRED) return null
    const newIpString = ips.join(',')
    return peerCacher.mutatePeer(deviceId, (p) => {
      if (p.ip !== newIpString) p.ip = newIpString
      if (p.port !== port) p.port = port
      if (p.name !== name) p.name = name
      if (p.deviceType !== deviceType) p.deviceType = deviceType
      p.updatedAt = new Date().toISOString()
    })
  }

  setOnlineStatus(peerId: string, online: boolean): void {
    peerCacher.setOnline(peerId, online)
  }

  async load(): Promise<void> {
    await peerCacher.load()
  }
}

export const peerManager = new PeerManager()
