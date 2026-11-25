import { showActionError } from '@/hooks/show-action-error'
import { chatContentInput } from '@/lib/chat/chat-content'
import { ref, shallowRef, watch, onUnmounted, type ComputedRef } from 'vue'
import { useI18n } from 'vue-i18n'
import { initQuery, chatItemsGQL } from '@/lib/api/query'
import { sendChatItemsGQL, deleteChatItemGQL, retryChatItemGQL, initMutation } from '@/lib/api/mutation'
import toast from '@/components/toaster'
import type { IChatItem } from '@/lib/interfaces'
import type { ChatTextFormat } from '@/lib/chat/message-draft'
import { ChatStatus } from '@/lib/status'
import { generateRecordId } from '@/lib/record-id.generated'
import { useTasks } from './chat'
import { useChatSubscriptions } from './chat-subscriptions'
import { normalizeChatItem } from './chat-subscriptions'

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
        chatItems.value = data.chatItems.map(normalizeChatItem).reverse()
        if (!initialized) { scrollBottom(); initialized = true }
      }
    },
    document: chatItemsGQL,
    variables: () => ({ query: chatId.value }),
  })

  const { mutate: sendMutate, loading: sendLoading } = initMutation({ document: sendChatItemsGQL })

  const { mutate: deleteItem, loading: deleteLoading, onDone: deleteDone } = initMutation({
    document: deleteChatItemGQL,
  }, showActionError)
  deleteDone(() => {
    chatItems.value = chatItems.value.filter((i) => i.id !== deleteId.value)
  })

  const { mutate: retryMutate } = initMutation({ document: retryChatItemGQL }, showActionError)

  async function doSend(tempId: string, toId: string, content: IChatItem['content']) {
    const r = await sendMutate({ targets: [toId], content: chatContentInput(content) })
    if (r?.data?.sendChatItems?.length) {
      if (toId === chatId.value) {
        const existing = chatItems.value.filter(item => item.id !== tempId)
        const ids = new Set(existing.map(item => item.id))
        const sent = r.data.sendChatItems.map(normalizeChatItem).filter((item: IChatItem) => !ids.has(item.id))
        chatItems.value = [...existing, ...sent]
        scrollBottom()
      }
      return true
    }
    const item = chatItems.value.find((i) => i.id === tempId)
    if (item) {
      chatItems.value = chatItems.value.filter((i) => i.id !== tempId)
      toast(t('send_failed'), 'error')
    }
    return false
  }

  async function send(text: string, format: ChatTextFormat) {
    if (!text.trim()) return false
    const tempId = 'new_' + generateRecordId()
    const tempItem: IChatItem = {
      id: tempId, fromId: 'me', toId: chatId.value, channelId: channelId.value,
      createdAt: new Date().toISOString(),
      content: { __typename: 'ChatMessageContent', text, format, items: [], linkPreviews: [] },
      deliveryResults: [],
      __typename: 'ChatItem',
      status: ChatStatus.PENDING,
    }
    chatItems.value = [...chatItems.value, tempItem]
    scrollBottom()
    return doSend(tempId, tempItem.toId, tempItem.content)
  }

  async function retryMessage(id: string) {
    const item = chatItems.value.find((i) => i.id === id)
    if (!item || id.startsWith('new_')) return
    chatItems.value = chatItems.value.map((i) => i.id === id ? { ...i, status: ChatStatus.PENDING } : i)
    const r = await retryMutate({ id })
    if (r?.data?.retryChatItem) {
      const updated = normalizeChatItem(r.data.retryChatItem)
      chatItems.value = chatItems.value.map((i) => i.id === id ? { ...i, ...updated } : i)
    } else {
      chatItems.value = chatItems.value.map((i) => i.id === id ? { ...i, status: ChatStatus.FAILED } : i)
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
  useChatSubscriptions(chatId, chatItems, scrollBottom)

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
