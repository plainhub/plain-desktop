import { ref, shallowRef, watch, onUnmounted, type ComputedRef, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { initQuery, chatItemsGQL } from '@/lib/api/query'
import { sendChatItemGQL, deleteChatItemGQL, retryChatItemGQL, initMutation } from '@/lib/api/mutation'
import toast from '@/components/toaster'
import type { IChatItem } from '@/lib/interfaces'
import { ChatStatus, MessageType } from '@/lib/status'
import { shortUUID } from '@/lib/strutil'
import { useTasks } from './chat'
import { useChatEvents } from './chat-events'
import { normalizeChatItem } from './chat-events'

export function useChatMessages(chatId: ComputedRef<string>, channelId: ComputedRef<string>) {
  const { t } = useI18n()
  const scrollContainer = ref<HTMLDivElement>()
  // shallowRef: items are treated as immutable — every mutation replaces the
  // array (or the item object) so a full-conversation list never pays deep
  // reactivity proxy cost.
  const chatItems = shallowRef<IChatItem[]>([])
  const { cancel: cancelTask } = useTasks()
  const deleteId = ref('')
  let initialized = false
  let isMounted = true

  function scrollBottom() {
    const div = scrollContainer.value
    if (!div) return
    setTimeout(() => { div.scrollTop = div.scrollHeight }, 100)
  }

  const { loading, refetch } = initQuery({
    handle: async (data: any, error: string) => {
      if (error) {
        toast(t(error), 'error')
      } else if (data) {
        chatItems.value = data.chatItems.map(normalizeChatItem)
        if (!initialized) { scrollBottom(); initialized = true }
      }
    },
    document: chatItemsGQL,
    variables: () => ({ id: chatId.value }),
  })

  const { mutate: sendMutate, loading: sendLoading, onDone: sendDone } = initMutation({
    document: sendChatItemGQL,
  }, false)
  sendDone((r: any) => {
    if (r?.data?.sendChatItem) {
      const rawItems: any[] = Array.isArray(r.data.sendChatItem) ? r.data.sendChatItem : [r.data.sendChatItem]
      const existingIds = new Set(chatItems.value.map((i) => i.id))
      const newItems = rawItems.map(normalizeChatItem).filter((i) => !existingIds.has(i.id))
      if (newItems.length) {
        chatItems.value = [...chatItems.value, ...newItems]
      }
    }
    scrollBottom()
  })

  const { mutate: deleteItem, loading: deleteLoading, onDone: deleteDone } = initMutation({
    document: deleteChatItemGQL,
  })
  deleteDone(() => {
    chatItems.value = chatItems.value.filter((i) => i.id !== deleteId.value)
  })

  const { mutate: retryMutate } = initMutation({ document: retryChatItemGQL })

  const MAX_SEND_ATTEMPTS = 3

  async function doSend(tempId: string, toId: string, content: string) {
    for (let attempt = 1; attempt <= MAX_SEND_ATTEMPTS; attempt++) {
      const r = await sendMutate({ toId, content })
      if (r != null) {
        chatItems.value = chatItems.value.filter((i) => i.id !== tempId)
        return
      }
      if (attempt < MAX_SEND_ATTEMPTS) {
        await new Promise((resolve) => setTimeout(resolve, attempt * 1000))
      }
    }
    const item = chatItems.value.find((i) => i.id === tempId)
    if (item) {
      chatItems.value = chatItems.value.map((i) => i.id === tempId ? { ...i, status: ChatStatus.FAILED } : i)
      toast(t('send_failed'), 'error')
    }
  }

  function send(chatText: Ref<string>) {
    if (!chatText.value) return
    const tempId = 'new_' + shortUUID()
    const tempItem: IChatItem = {
      id: tempId, fromId: 'me', toId: chatId.value, channelId: channelId.value,
      createdAt: new Date().toISOString(),
      content: JSON.stringify({ type: MessageType.TEXT, value: { text: chatText.value } }),
      _content: { type: MessageType.TEXT, value: { text: chatText.value } },
      __typename: 'ChatItem',
      data: { ids: [] },
      status: ChatStatus.PENDING,
    }
    chatItems.value = [...chatItems.value, tempItem]
    chatText.value = ''
    scrollBottom()
    doSend(tempId, tempItem.toId, tempItem.content)
  }

  async function retryMessage(id: string) {
    const item = chatItems.value.find((i) => i.id === id)
    if (!item) return
    chatItems.value = chatItems.value.map((i) => i.id === id ? { ...i, status: ChatStatus.PENDING } : i)
    if (id.startsWith('new_')) {
      // Network error: item was never saved on server → resend as new message
      await doSend(id, chatId.value, item.content)
    } else {
      // Delivery failure: item exists on server → retry delivery in-place
      const r = await retryMutate({ id })
      if (r?.data?.retryChatItem) {
        const updated = normalizeChatItem(r.data.retryChatItem)
        chatItems.value = chatItems.value.map((i) => i.id === id ? { ...i, ...updated } : i)
      } else {
        chatItems.value = chatItems.value.map((i) => i.id === id ? { ...i, status: ChatStatus.FAILED } : i)
      }
    }
  }

  function deleteMessage(id: string) {
    if (id.startsWith('new_')) {
      chatItems.value = chatItems.value.filter((i) => i.id !== id)
      cancelTask(id)
      return
    }
    deleteId.value = id
    deleteItem({ id })
    cancelTask(id)
  }

  async function clearMessages() {
    const ids = chatItems.value.filter((i) => !i.id.startsWith('new_')).map((i) => i.id)
    for (const id of ids) {
      cancelTask(id)
      deleteId.value = id
      await deleteItem({ id })
    }
  }

  // Real-time event bus handlers for message CRUD
  useChatEvents(chatId, chatItems, scrollBottom)

  watch(chatId, () => {
    if (!isMounted) return
    initialized = false
    chatItems.value = []
    scrollBottom()
  })

  onUnmounted(() => {
    isMounted = false
  })

  return {
    chatItems, loading, sendLoading, deleteLoading,
    scrollContainer, scrollBottom,
    send, retryMessage, deleteMessage, clearMessages, refetch,
  }
}
