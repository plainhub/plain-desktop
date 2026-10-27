import type { ComputedRef, Ref } from 'vue'
import { onMounted, onUnmounted } from 'vue'
import type { SubscriptionResults } from '@/lib/api/subscription-results'
import type { IChatItem } from '@/lib/interfaces'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'

export function normalizeChatItem(item: IChatItem | SubscriptionResults['messagesCreated'][number]): IChatItem {
  const content = '_content' in item ? item._content : null
  return {
    ...item,
    _content: content ?? (item.content ? JSON.parse(item.content) : null),
    __typename: 'ChatItem',
  }
}

export function useChatSubscriptions(chatId: ComputedRef<string>, chatItems: Ref<IChatItem[]>, scrollBottom: () => void) {
  const stops: (() => void)[] = []

  onMounted(() => {
    const onCreated = (data: SubscriptionResults['messagesCreated']) => {
      const peerId = chatId.value.startsWith('peer:') ? chatId.value.slice(5) : ''
      const items = data
        .filter((item) => {
          const id = item.channelId ? `channel:${item.channelId}` : item.toId === 'local' ? 'peer:local' : `peer:${item.toId}`
          return id === chatId.value || item.fromId === peerId
        })
        .map(normalizeChatItem)
      if (!items.length) return
      const existingIds = new Set(chatItems.value.map((i: any) => i.id))
      const newItems = items.filter((i) => !existingIds.has(i.id))
      if (newItems.length) { chatItems.value = [...chatItems.value, ...newItems]; scrollBottom() }
    }
    stops.push(graphqlSubscriptions.subscribe('messagesCreated', onCreated))

    const onDeleted = (data: string) => {
      if (data.startsWith('ids=')) {
        const ids = data.slice(4).split(',').filter(Boolean)
        if (!ids.length) return
        const idSet = new Set(ids)
        chatItems.value = chatItems.value.filter((i) => !idSet.has(i.id))
      } else if (data === chatId.value) {
        chatItems.value = []
      }
    }
    stops.push(graphqlSubscriptions.subscribe('messagesDeleted', onDeleted))

    const onUpdated = (items: SubscriptionResults['messagesUpdated']) => {
      const updateMap = new Map(items.map((item) => [item.id, normalizeChatItem(item)]))
      chatItems.value = chatItems.value.map((i) => updateMap.has(i.id) ? { ...i, ...updateMap.get(i.id)! } : i)
      if (updateMap.size) scrollBottom()
    }
    stops.push(graphqlSubscriptions.subscribe('messagesUpdated', onUpdated))
  })

  onUnmounted(() => {
    stops.forEach(stop => stop())
  })
}
