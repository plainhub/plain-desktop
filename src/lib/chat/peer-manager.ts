import { initMutation, deletePeerGQL, unpairPeerGQL } from '@/lib/api/mutation'
import { peerCacher } from './peer-cacher'
import { chatCacher } from './chat-cacher'

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

  setOnlineStatus(peerId: string, online: boolean): void {
    peerCacher.setOnline(peerId, online)
  }

  async load(): Promise<void> {
    await peerCacher.load()
  }
}

export const peerManager = new PeerManager()
