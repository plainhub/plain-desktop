import { ref } from 'vue'
import { initLazyQuery, chatConversationsGQL } from '@/lib/api/query'
import type { IChatItem } from '@/lib/interfaces'
import { getDesktopClientId } from '@/lib/device/client-id'
import { peerCacher } from './peer-cacher'
import { channelCacher } from './channel-cacher'

export function normalizeChatItem(item: any): IChatItem {
  let parsedContent: IChatItem['_content']
  try {
    parsedContent = item?._content ?? (item?.content ? JSON.parse(item.content) : null)
  } catch {
    parsedContent = null
  }
  return {
    ...item,
    _content: parsedContent,
    __typename: item?.__typename ?? 'ChatItem',
  }
}

export function getLocalClientId(): string {
  return getDesktopClientId()
}

export function isLocalEndpoint(id: string, localClientId: string): boolean {
  return id === '' || id === 'local' || (!!localClientId && id === localClientId)
}

export function mapChatId(
  item: IChatItem,
  peerIds: Set<string>,
  channelIds: Set<string>,
): string | null {
  if (item.channelId && channelIds.has(item.channelId)) return `channel:${item.channelId}`
  const fromId = item.fromId ?? ''
  const toId = item.toId ?? ''
  const localClientId = getLocalClientId()
  const isFromPeer = !!fromId && peerIds.has(fromId)
  const isToPeer = !!toId && peerIds.has(toId)
  const isFromLocal = isLocalEndpoint(fromId, localClientId)
  const isToLocal = isLocalEndpoint(toId, localClientId)
  if (isToPeer) return `peer:${toId}`
  if (isFromPeer) return `peer:${fromId}`
  if (isFromLocal || isToLocal) return 'peer:local'
  if (toId) return `peer:${toId}`
  if (fromId) return `peer:${fromId}`
  return 'peer:local'
}

class ChatCacher {
  readonly latestChatMap = ref<Record<string, IChatItem>>({})

  private readonly lazy = initLazyQuery<{ chatConversations: Array<{ id: string; chat: IChatItem }> }>({
    handle: (data) => {
      if (!data?.chatConversations) return
      this.applyConversations(data.chatConversations)
    },
    document: chatConversationsGQL,
    variables: () => ({}),
  })

  get loading() {
    return this.lazy.loading
  }

  getLatestChat(chatId: string): IChatItem | null {
    return this.latestChatMap.value[chatId] ?? null
  }

  async load(): Promise<void> {
    await this.lazy.fetch()
  }

  upsertLatest(item: IChatItem): void {
    const { peerIds, channelIds } = this.collectIds()
    const chatId = mapChatId(item, peerIds, channelIds)
    if (!chatId) return
    this.latestChatMap.value[chatId] = item
  }

  updateLatestIfPresent(item: IChatItem): boolean {
    const { peerIds, channelIds } = this.collectIds()
    const chatId = mapChatId(item, peerIds, channelIds)
    if (!chatId) return false
    if (this.latestChatMap.value[chatId]?.id !== item.id) return false
    this.latestChatMap.value[chatId] = item
    return true
  }

  removeLatest(chatId: string): void {
    if (!this.latestChatMap.value[chatId]) return
    const next = { ...this.latestChatMap.value }
    delete next[chatId]
    this.latestChatMap.value = next
  }

  setLatestMap(map: Record<string, IChatItem>): void {
    this.latestChatMap.value = map
  }

  private collectIds(): { peerIds: Set<string>; channelIds: Set<string> } {
    return {
      peerIds: new Set(peerCacher.peersMap.value.keys()),
      channelIds: new Set(channelCacher.channelsMap.value.keys()),
    }
  }

  private applyConversations(rows: Array<{ id: string; chat: IChatItem }>): void {
    this.latestChatMap.value = Object.fromEntries(rows.map(({ id, chat }) => [
      `${chat.channelId ? 'channel' : 'peer'}:${id}`, normalizeChatItem(chat),
    ]))
  }

}

export const chatCacher = new ChatCacher()
