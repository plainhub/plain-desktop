import {
  acceptChatChannelInviteGQL,
  addChatChannelMemberGQL,
  createChatChannelGQL,
  declineChatChannelInviteGQL,
  deleteChatChannelGQL,
  initMutation,
  leaveChatChannelGQL,
  removeChatChannelMemberGQL,
  updateChatChannelGQL,
} from '@/lib/api/mutation'
import { channelCacher } from './channel-cacher'
import type { IChatChannel } from '@/lib/interfaces'

class ChannelManager {
  private readonly createMut = initMutation({ document: createChatChannelGQL })
  private readonly renameMut = initMutation({ document: updateChatChannelGQL })
  private readonly deleteMut = initMutation({ document: deleteChatChannelGQL })
  private readonly leaveMut = initMutation({ document: leaveChatChannelGQL })
  private readonly inviteMut = initMutation({ document: addChatChannelMemberGQL })
  private readonly kickMut = initMutation({ document: removeChatChannelMemberGQL })
  private readonly acceptMut = initMutation({ document: acceptChatChannelInviteGQL })
  private readonly declineMut = initMutation({ document: declineChatChannelInviteGQL })

  async createChannel(name: string): Promise<IChatChannel | null> {
    const r = await this.createMut.mutate({ name })
    if (!r) return null
    await channelCacher.load()
    return r.data?.createChatChannel ?? null
  }

  async renameChannel(channelId: string, newName: string): Promise<IChatChannel | null> {
    const r = await this.renameMut.mutate({ id: channelId, name: newName })
    if (!r) return null
    await channelCacher.load()
    return r.data?.updateChatChannel ?? null
  }

  async deleteChannel(channelId: string): Promise<boolean> {
    const r = await this.deleteMut.mutate({ id: channelId })
    if (!r) return false
    channelCacher.removeChannel(channelId)
    return true
  }

  async leaveChannel(channelId: string): Promise<boolean> {
    const r = await this.leaveMut.mutate({ id: channelId })
    if (!r) return false
    channelCacher.removeChannel(channelId)
    return true
  }

  async inviteMember(channelId: string, peerId: string): Promise<IChatChannel | null> {
    const r = await this.inviteMut.mutate({ id: channelId, peerId })
    if (!r) return null
    await channelCacher.load()
    return r.data?.addChatChannelMember ?? null
  }

  async kickMember(channelId: string, peerId: string): Promise<IChatChannel | null> {
    const r = await this.kickMut.mutate({ id: channelId, peerId })
    if (!r) return null
    await channelCacher.load()
    return r.data?.removeChatChannelMember ?? null
  }

  async acceptInvite(channelId: string): Promise<boolean> {
    const r = await this.acceptMut.mutate({ id: channelId })
    if (!r) return false
    await channelCacher.load()
    return true
  }

  async declineInvite(channelId: string): Promise<boolean> {
    const r = await this.declineMut.mutate({ id: channelId })
    if (!r) return false
    channelCacher.removeChannel(channelId)
    return true
  }
}

export const channelManager = new ChannelManager()
