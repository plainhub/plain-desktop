import { ref } from 'vue'
import { initLazyQuery, latestChatItemsGQL } from '@/lib/api/query'
import type { IChatItem } from '@/lib/interfaces'
import { get as prefsGet } from '@/lib/prefs'
import { peerCacher } from './peer-cacher'
import { channelCacher } from './channel-cacher'

export function normalizeChatItem(item: any): IChatItem {
  let parsedContent = null
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
  return prefsGet('client_id', '')
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

  private readonly lazy = initLazyQuery<{ latestChatItems: IChatItem[] }>({
    handle: (data) => {
      if (!data?.latestChatItems) return
      this.applyLatest(data.latestChatItems)
    },
    document: latestChatItemsGQL,
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

  private applyLatest(items: IChatItem[]): void {
    const { peerIds, channelIds } = this.collectIds()
    const next: Record<string, IChatItem> = {}
    for (const raw of items) {
      const item = normalizeChatItem(raw)
      const chatId = mapChatId(item, peerIds, channelIds)
      if (!chatId) continue
      const existing = next[chatId]
      if (!existing || item.createdAt > existing.createdAt) {
        next[chatId] = item
      }
    }
    this.latestChatMap.value = next
  }
}

export const chatCacher = new ChatCacher()
