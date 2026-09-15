import { defineStore } from 'pinia'
import { computed } from 'vue'
import type { IPeer, IChatChannel, IChatItem } from '@/lib/interfaces'
import { ChannelStatus, MessageType } from '@/lib/status'
import emitter from '@/plugins/eventbus'
import { getCached, setCached } from '@/lib/api/cache'
import { openModal } from '@/components/modal'
import ChannelInviteModal from '@/views/chat/ChannelInviteModal.vue'
import { chatCacher, normalizeChatItem } from '@/lib/chat/chat-cacher'
import { peerCacher } from '@/lib/chat/peer-cacher'
import { channelCacher } from '@/lib/chat/channel-cacher'
import { i18n } from '@/plugins/i18n'

export const useChatStore = defineStore('chat', () => {
  const peers = computed(() => {
    const online = peerCacher.onlineMap.value
    return Array.from(peerCacher.peersMap.value.values()).map((r) => ({
      ...r.peer,
      online: online.get(r.peer.id) ?? r.peer.online ?? false,
    }))
  })

  const channels = computed(() => channelCacher.channels.value)
  const latestChatMap = chatCacher.latestChatMap
  const loading = peerCacher.loading

  const pairedPeers = computed(() => peerCacher.pairedPeers.value)
  const unpairedPeers = computed(() => peerCacher.unpairedPeers.value)
  const allPeers = computed(() => [...pairedPeers.value, ...unpairedPeers.value])
  const joinedChannels = computed(() => channels.value.filter((c) => c.status === ChannelStatus.JOINED))

  function findPeer(id: string): IPeer | null {
    return peerCacher.getPeer(id)
  }

  function findChannel(id: string): IChatChannel | null {
    return channelCacher.getChannel(id)
  }

  function updateChannel(channel: IChatChannel) {
    channelCacher.upsertChannel(channel)
    setCached<IChatChannel[]>('chat:channels', channelCacher.channels.value)
  }

  function removeChannel(channelId: string) {
    if (!channelCacher.channelsMap.value.has(channelId)) return
    channelCacher.removeChannel(channelId)
    setCached<IChatChannel[]>('chat:channels', channelCacher.channels.value)
  }

  function getLatestChat(chatId: string): IChatItem | null {
    return chatCacher.getLatestChat(chatId)
  }

  function getLatestChatPreview(chatId: string): string {
    const item = getLatestChat(chatId)
    if (!item) return ''
    const content = item._content
    const t = i18n.global.t
    if (content?.type === MessageType.TEXT) {
      const text = content?.value?.text
      if (text) return text.slice(0, 50)
      return t('message')
    }
    if (content?.type === MessageType.IMAGES) {
      const items = content?.value?.items ?? []
      const videoCount = items.filter((it: any) => it.duration > 0).length
      const imageCount = items.length - videoCount
      if (imageCount > 0 && videoCount > 0) {
        const imgPart = imageCount > 1 ? `${imageCount} ${t('images')}` : t('image')
        const vidPart = videoCount > 1 ? `${videoCount} ${t('videos')}` : t('video')
        return `${imgPart}, ${vidPart}`
      }
      if (videoCount > 0) {
        return videoCount > 1 ? `${videoCount} ${t('videos')}` : t('video')
      }
      return imageCount > 1 ? `${imageCount} ${t('images')}` : t('image')
    }
    if (content?.type === MessageType.FILES) {
      const count = content?.value?.items?.length ?? 0
      return count > 1 ? `${count} ${t('files')}` : t('file')
    }
    if (content?.type === MessageType.SHARE) {
      return t('shared_folder')
    }
    return t('message')
  }

  function getLatestChatCreatedAt(chatId: string): string {
    return getLatestChat(chatId)?.createdAt ?? ''
  }

  function peersCacheValue(): IPeer[] {
    const online = peerCacher.onlineMap.value
    return Array.from(peerCacher.peersMap.value.values()).map((r) => ({
      ...r.peer,
      online: online.get(r.peer.id) ?? r.peer.online ?? false,
    }))
  }

  emitter.on('channels_updated', (data: any[]) => {
    if (!data) return
    const list = data.map((c: any) => ({ ...c }) as IChatChannel)
    channelCacher.replaceChannels(list)
    setCached<IChatChannel[]>('chat:channels', channelCacher.channels.value)
  })

  emitter.on('peer_status_updated', (data) => {
    if (!data?.id) return
    peerCacher.setOnline(data.id, !!data.online)
    setCached<IPeer[]>('chat:peers', peersCacheValue())
  })

  emitter.on('channel_invite_received', (data) => {
    if (!data?.channelId) return
    const invite = {
      channelId: data.channelId,
      channelName: data.channelName ?? '',
      fromId: data.fromId ?? '',
      fromName: data.fromName ?? '',
    }
    console.debug('[chat] channel_invite_received', invite)
    try {
      openModal(ChannelInviteModal, {
        invite,
        onResponded: (channelId: string, accepted: boolean) => {
          if (!accepted) {
            removeChannel(channelId)
          }
        },
      })
    } catch (e) {
      console.error('[chat] failed to open channel invite modal', e)
      setTimeout(() => {
        try {
          openModal(ChannelInviteModal, {
            invite,
            onResponded: (channelId: string, accepted: boolean) => {
              if (!accepted) removeChannel(channelId)
            },
          })
        } catch (e2) {
          console.error('[chat] deferred openModal also failed', e2)
        }
      }, 500)
    }
  })

  emitter.on('message_created', (items: any[]) => {
    if (!Array.isArray(items)) return
    items.map(normalizeChatItem).forEach((item) => {
      chatCacher.upsertLatest(item)
    })
    setCached<Record<string, IChatItem>>('chat:latest_items', chatCacher.latestChatMap.value)
  })

  emitter.on('message_updated', (items: any[]) => {
    if (!Array.isArray(items)) return
    items.map(normalizeChatItem).forEach((item) => {
      chatCacher.updateLatestIfPresent(item)
    })
    setCached<Record<string, IChatItem>>('chat:latest_items', chatCacher.latestChatMap.value)
  })

  emitter.on('message_deleted', (data: string) => {
    if (typeof data !== 'string') return
    if (data.startsWith('ids=')) {
      const ids = data.slice(4).split(',').filter(Boolean)
      if (!ids.length) return
      const deleted = new Set(ids)
      const hasLatestDeleted = Object.values(chatCacher.latestChatMap.value).some((item) => deleted.has(item.id))
      if (hasLatestDeleted) fetchLatestChatItems()
    } else if (chatCacher.latestChatMap.value[data]) {
      chatCacher.removeLatest(data)
      setCached<Record<string, IChatItem>>('chat:latest_items', chatCacher.latestChatMap.value)
    }
  })

  async function fetchPeers() {
    await peerCacher.load()
    setCached<IPeer[]>('chat:peers', peersCacheValue())
    await chatCacher.load()
    setCached<Record<string, IChatItem>>('chat:latest_items', chatCacher.latestChatMap.value)
  }

  async function fetchChannels() {
    await channelCacher.load()
    setCached<IChatChannel[]>('chat:channels', channelCacher.channels.value)
  }

  async function fetchLatestChatItems() {
    await chatCacher.load()
    setCached<Record<string, IChatItem>>('chat:latest_items', chatCacher.latestChatMap.value)
  }

  function init() {
    const cachedPeers = getCached<IPeer[]>('chat:peers')
    if (cachedPeers) peerCacher.replacePeers(cachedPeers)

    const cachedChannels = getCached<IChatChannel[]>('chat:channels')
    if (cachedChannels) channelCacher.replaceChannels(cachedChannels)

    const cachedLatest = getCached<Record<string, IChatItem>>('chat:latest_items')
    if (cachedLatest) chatCacher.setLatestMap(cachedLatest)

    fetchPeers()
    fetchChannels()
    fetchLatestChatItems()
  }

  return {
    peers,
    channels,
    latestChatMap,
    loading,
    pairedPeers,
    unpairedPeers,
    allPeers,
    joinedChannels,
    findPeer,
    findChannel,
    updateChannel,
    removeChannel,
    getLatestChat,
    getLatestChatPreview,
    getLatestChatCreatedAt,
    fetchPeers,
    fetchChannels,
    fetchLatestChatItems,
    init,
  }
})
