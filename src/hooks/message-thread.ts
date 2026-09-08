import { computed, nextTick, ref, type Ref } from 'vue'
import toast from '@/components/toaster'
import { initLazyQuery, smsGQL, type QueryResponseContext } from '@/lib/api/query'
import { useI18n } from 'vue-i18n'
import { buildQuery } from '@/lib/search'
import type { IItemTagsUpdatedEvent, IItemsTagsUpdatedEvent, IMessage, IMessageConversation, IMmsSendResultEvent, ISmsSendResultEvent } from '@/lib/interfaces'
import { useTags } from '@/hooks/tags'
import { useContactName } from '@/hooks/contacts'
import { DataType } from '@/lib/data'
import emitter from '@/plugins/eventbus'
import { createPendingMms, sortByDate } from '@/lib/message-helpers'
import { createSmsNotificationRefresh } from '@/hooks/sms-notification-refresh'
import {
  addPendingMms,
  addPendingSms,
  failPendingSms,
  isPendingSmsSent,
  reconcilePendingSms,
  settlePendingMms,
  settlePendingSmsResult,
  visiblePendingSms,
} from '@/lib/sms-state-sync'
import { getConversationAddresses } from '@/lib/contact/name-resolution'
import { shortUUID } from '@/lib/strutil'
import { createKeyedSmsSendDeadlines, SMS_SEND_RESULT_TIMEOUT_MS } from '@/lib/sms-send-deadline'
import { takeMmsSendResult, takeSmsSendResult } from '@/lib/sms-result-ledger'

const PAGE_SIZE = 100
type ThreadRequestMeta = { threadId: string; mode: 'reset' | 'more' }

export function useMessageThread(
  threadId: Ref<string>,
  chatScrollRef: Ref<HTMLElement | undefined>,
  isArchived?: Ref<boolean>,
  conversation?: Ref<IMessageConversation | undefined>,
) {
  const { t } = useI18n()
  const { loadContacts, getDisplayName } = useContactName()
  const { tags, fetch: fetchTags } = useTags(DataType.SMS)

  const items = ref<IMessage[]>([])
  const detailLoading = ref(false)
  const noMoreOlder = ref(false)
  const loadingMore = ref(false)
  const pendingMmsItems = ref<IMessage[]>([])
  const pendingSmsItems = ref<IMessage[]>([])
  const retryTimers = new Map<string, Set<ReturnType<typeof setTimeout>>>()
  const pendingMmsTimers = new Map<string, ReturnType<typeof setTimeout>>()
  let onTerminalSmsFailure: ((failed: IMessage) => void) | undefined
  let onTerminalMmsResult: ((pendingId: string, success: boolean) => void) | undefined

  const participantAddresses = computed(() => {
    if (conversation?.value) return getConversationAddresses(conversation.value)
    const address = sortByDate(items.value, true)
      .find((item) => item.address)?.address
    return address ? [address] : []
  })

  const contactName = computed(() => getDisplayName(participantAddresses.value))
  const contactAddress = computed(() => participantAddresses.value.join(', '))

  const sortedItems = computed(() => {
    const base = sortByDate(items.value)
    const baseIds = new Set(base.map((item) => item.id))
    const pending = visiblePendingSms(pendingSmsItems.value, threadId.value)
      .filter((operation) => !baseIds.has(operation.id))
    pending.push(...pendingMmsItems.value.filter((item) =>
      item.threadId === threadId.value && !baseIds.has(item.id),
    ))
    return pending.length ? [...base, ...pending] : base
  })

  function scrollToBottom() {
    nextTick(() => { if (chatScrollRef.value) chatScrollRef.value.scrollTop = chatScrollRef.value.scrollHeight })
  }

  const { loading, fetch: rawFetch } = initLazyQuery({
    handle: (data: { sms: IMessage[]; smsCount: number }, error: string, context?: QueryResponseContext) => {
      const meta = context?.meta as ThreadRequestMeta | undefined
      if (!meta || meta.threadId !== threadId.value) return
      if (error) {
        detailLoading.value = false
        loadingMore.value = false
        toast(t(error), 'error')
        return
      }
      if (!data) return

      if (meta.mode === 'more') {
        const el = chatScrollRef.value
        const prevScrollHeight = el?.scrollHeight ?? 0
        const existingIds = new Set(items.value.map((item) => item.id))
        items.value = [...data.sms.filter((item) => !existingIds.has(item.id)), ...items.value]
        if (data.sms.length < PAGE_SIZE) noMoreOlder.value = true
        loadingMore.value = false
        nextTick(() => { if (el) el.scrollTop = el.scrollHeight - prevScrollHeight })
      } else {
        detailLoading.value = false
        items.value = data.sms
        noMoreOlder.value = data.sms.length < PAGE_SIZE
        const previousPendingIds = new Set(pendingSmsItems.value.map((item) => item.id))
        pendingSmsItems.value = reconcilePendingSms(pendingSmsItems.value, data.sms, meta.threadId)
        const remainingPendingIds = new Set(pendingSmsItems.value.map((item) => item.id))
        for (const pendingId of previousPendingIds) {
          if (!remainingPendingIds.has(pendingId)) {
            smsDeadlines.settle(pendingId)
            cancelRetry(pendingId)
          }
        }
        scrollToBottom()
      }
    },
    document: smsGQL,
  })

  function variables(offset: number) {
    const fields = [{ name: 'thread_id', op: '', value: threadId.value }]
    if (isArchived?.value) fields.push({ name: 'archived', op: '', value: '1' })
    return { offset, limit: PAGE_SIZE, query: buildQuery(fields) }
  }

  function fetch(force = false) {
    if (!threadId.value) return Promise.resolve()
    noMoreOlder.value = false
    loadingMore.value = false
    return rawFetch(variables(0), {
      force,
      latest: true,
      meta: { threadId: threadId.value, mode: 'reset' } satisfies ThreadRequestMeta,
    })
  }

  function fetchMore() {
    if (loadingMore.value || noMoreOlder.value || loading.value || !threadId.value) return
    loadingMore.value = true
    return rawFetch(variables(items.value.length), {
      latest: true,
      meta: { threadId: threadId.value, mode: 'more' } satisfies ThreadRequestMeta,
    })
  }

  function onScroll() {
    if (!chatScrollRef.value || loadingMore.value || noMoreOlder.value || loading.value) return
    if (chatScrollRef.value.scrollTop < 200) void fetchMore()
  }

  function cancelRetry(requestId: string) {
    retryTimers.get(requestId)?.forEach((timer) => clearTimeout(timer))
    retryTimers.delete(requestId)
  }

  function cancelRetries() {
    for (const requestId of retryTimers.keys()) cancelRetry(requestId)
  }

  function refetchWithRetry(requestId: string) {
    cancelRetry(requestId)
    const delays = [1000, 2000, 3000]
    const timers = new Set<ReturnType<typeof setTimeout>>()
    retryTimers.set(requestId, timers)
    const run = async (attempt: number) => {
      if (!pendingSmsItems.value.some((item) => item.id === requestId)) return cancelRetry(requestId)
      await fetch(true)
      if (!pendingSmsItems.value.some((item) => item.id === requestId)) return cancelRetry(requestId)
      if (attempt + 1 < delays.length) {
        const timer = setTimeout(() => void run(attempt + 1), delays[attempt + 1])
        timers.add(timer)
      }
    }
    const timer = setTimeout(() => void run(0), delays[0])
    timers.add(timer)
  }

  function setPendingSms(body: string, address: string, requestId = `pending_sms_${shortUUID()}`) {
    pendingSmsItems.value = addPendingSms(
      pendingSmsItems.value,
      requestId,
      body,
      address,
      threadId.value,
      new Date(),
      items.value.map((item) => item.id),
    )
    scrollToBottom()
    return requestId
  }

  function startPendingSmsDeadline(requestId: string) {
    if (pendingSmsItems.value.some((item) => item.id === requestId && !isPendingSmsSent(item))) {
      smsDeadlines.start(requestId)
    }
  }

  function handleSmsSendResult(result: ISmsSendResultEvent): { handled: boolean; failed?: IMessage } {
    const outcome = settlePendingSmsResult(pendingSmsItems.value, result)
    if (!outcome.handled || !result.requestId) return { handled: false }
    if (!outcome.transitioned) return { handled: true }
    pendingSmsItems.value = outcome.pending
    smsDeadlines.settle(result.requestId)
    if (result.success) {
      refetchWithRetry(result.requestId)
      return { handled: true }
    }
    cancelRetry(result.requestId)
    toast(t('send_failed'), 'error')
    return { handled: true, failed: outcome.failed }
  }

  function failPending(requestId: string): IMessage | undefined {
    const failed = failPendingSms(pendingSmsItems.value, requestId)
    pendingSmsItems.value = failed.pending
    smsDeadlines.settle(requestId)
    cancelRetry(requestId)
    return failed.failed
  }

  function setPendingMms(id: string, body: string, address: string, attachments: IMessage['attachments']) {
    pendingMmsItems.value = addPendingMms(
      pendingMmsItems.value,
      createPendingMms(id, body, address, threadId.value, attachments),
    )
    const existingTimer = pendingMmsTimers.get(id)
    if (existingTimer) clearTimeout(existingTimer)
    pendingMmsTimers.set(id, setTimeout(() => {
      pendingMmsTimers.delete(id)
      const queued = takeMmsSendResult(id)
      const result = queued ?? { pendingId: id, success: false, resultCode: -1001 }
      const outcome = handleMmsSendResult(result)
      if (outcome.handled) onTerminalMmsResult?.(id, result.success)
    }, SMS_SEND_RESULT_TIMEOUT_MS))
    scrollToBottom()
  }

  function onMmsSent(pendingId: string) {
    const settled = settlePendingMms(pendingMmsItems.value, pendingId)
    if (!settled.settled) return
    pendingMmsItems.value = settled.pending
    const timer = pendingMmsTimers.get(pendingId)
    if (timer) clearTimeout(timer)
    pendingMmsTimers.delete(pendingId)
  }

  function handleMmsSendResult(result: IMmsSendResultEvent): { handled: boolean; failed?: IMessage } {
    const settled = settlePendingMms(pendingMmsItems.value, result.pendingId)
    if (!settled.settled) return { handled: false }
    pendingMmsItems.value = settled.pending
    const timer = pendingMmsTimers.get(result.pendingId)
    if (timer) clearTimeout(timer)
    pendingMmsTimers.delete(result.pendingId)
    if (!result.success) {
      toast(t('send_failed'), 'error')
      return { handled: true, failed: settled.settled }
    }
    return { handled: true }
  }

  const smsDeadlines = createKeyedSmsSendDeadlines((requestId) => {
    const queued = takeSmsSendResult(requestId)
    const outcome = handleSmsSendResult(queued ?? { requestId, success: false, resultCode: -1001 })
    if (outcome.failed) onTerminalSmsFailure?.(outcome.failed)
  })

  function setTerminalHandlers(handlers: {
    onSmsFailure: (failed: IMessage) => void
    onMmsResult: (pendingId: string, success: boolean) => void
  }) {
    onTerminalSmsFailure = handlers.onSmsFailure
    onTerminalMmsResult = handlers.onMmsResult
  }

  function applyThread(tid: string, force = false) {
    cancelRetries()
    threadId.value = tid
    if (!tid) { items.value = []; detailLoading.value = false; return }
    items.value = []
    detailLoading.value = true
    void fetch(force)
  }

  const onItemsTagsUpdated = (event: IItemsTagsUpdatedEvent) => { if (event.type === DataType.SMS) void fetch(true) }
  const onItemTagsUpdated = (event: IItemTagsUpdatedEvent) => { if (event.type === DataType.SMS) void fetch(true) }
  const stateRefresh = createSmsNotificationRefresh(() => void fetch(true), () => loadContacts(true))

  function subscribe(force = false) {
    fetchTags()
    loadContacts(force)
    emitter.on('item_tags_updated', onItemTagsUpdated)
    emitter.on('items_tags_updated', onItemsTagsUpdated)
    emitter.on('mms_sent', onMmsSent)
    stateRefresh.subscribe()
  }

  function unsubscribe() {
    emitter.off('item_tags_updated', onItemTagsUpdated)
    emitter.off('items_tags_updated', onItemsTagsUpdated)
    emitter.off('mms_sent', onMmsSent)
    stateRefresh.unsubscribe()
    cancelRetries()
  }

  return {
    items,
    sortedItems,
    pendingSmsItems,
    pendingMmsItems,
    detailLoading,
    loading,
    loadingMore,
    tags,
    contactName,
    contactAddress,
    participantAddresses,
    fetch,
    refetchWithRetry,
    onScroll,
    scrollToBottom,
    applyThread,
    setPendingSms,
    startPendingSmsDeadline,
    failPending,
    handleSmsSendResult,
    handleMmsSendResult,
    setPendingMms,
    setTerminalHandlers,
    subscribe,
    unsubscribe,
  }
}
