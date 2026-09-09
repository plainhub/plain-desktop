<template>
  <div class="content">
    <MessageChatHeader
      :name="thread.contactName.value"
      :address="thread.contactAddress.value"
      :show-notification-warning="!app.permissions.includes('NOTIFICATION_LISTENER')"
      @back="backToList"
      @export="openExport"
      @call="send.callContact"
      @archive="archiveConversation"
    />
    <MessageChatList
      v-model:scroll-ref="chatScrollRef"
      :items="thread.sortedItems.value"
      :detail-loading="thread.detailLoading.value"
      :loading="thread.loading.value"
      :loading-more="thread.loadingMore.value"
      :permissions="app.permissions"
      :url-token-key="urlTokenKey"
      :tags="thread.tags.value"
      :type="DataType.SMS"
      @scroll="thread.onScroll"
    />
    <MessageChatInput
      v-model="send.messageBody.value"
      v-model:file-input-ref="send.fileInputRef.value"
      :pending-files="send.pendingFiles.value"
      :total-pending-size="send.totalPendingSize.value"
      :has-large-non-image-file="send.hasLargeNonImageFile.value"
      :warn-size="send.MMS_WARN_SIZE"
      :send-disabled="send.sendDisabled.value || !sendAddress"
      :capture-disabled="send.sendDisabled.value || !captureEligible"
      :sims="send.sims.value"
      :selected-sim-id="send.selectedSimId.value"
      @send="onSend"
      @open-file-picker="send.openFilePicker"
      @file-selected="send.onFileSelected"
      @remove-file="send.removePendingFile"
      @request-capture="handleCaptureRequest"
      @update:selected-sim-id="
        (v) => {
          send.selectedSimId.value = v
          mainStore.selectedSimSubscriptionId = v
        }
      "
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onActivated, onDeactivated, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'
import { replacePath } from '@/plugins/router'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { openModal } from '@/components/modal'
import ExportSmsModal from '@/views/messages/ExportSmsModal.vue'
import { DataType } from '@/lib/data'
import { useMessageThread } from '@/hooks/message-thread'
import { useMessageSend } from '@/hooks/message-send'
import MessageChatHeader from '@/views/messages/MessageChatHeader.vue'
import MessageChatList from '@/views/messages/MessageChatList.vue'
import MessageChatInput from '@/views/messages/MessageChatInput.vue'
import { useSmsStore } from '@/stores/sms'
import emitter from '@/plugins/eventbus'
import type { IMmsSendResultEvent, ISmsSendResultEvent } from '@/lib/interfaces'
import { resolveConversationSendAddress } from '@/lib/sms-conversation-sync'
import toast from '@/components/toaster'
import type { ChatCaptureDestination, ChatCaptureTarget } from '@/lib/screen-capture/tauri-capture-adapter'
import { sendCapturedMms, snapshotMessageCaptureDestination } from './message-capture'
import { initLazyQuery, smsConversationsGQL, smsConversationsWithAddressesGQL, type QueryResponseContext } from '@/lib/api/query'
import { buildQuery } from '@/lib/search'
import type { IMessageConversation } from '@/lib/interfaces'
import { subscribeMmsSendResults, subscribeSmsSendResults, takeMmsSendResult, takeSmsSendResult } from '@/lib/sms-result-ledger'

const mainStore = useMainStore()
const { t } = useI18n()
const { app, urlTokenKey } = storeToRefs(useTempStore())
const route = useRoute()
const smsStore = useSmsStore()
const { conversations, participantFieldsSupported } = storeToRefs(smsStore)
const threadId = ref('')
const chatScrollRef = ref<HTMLElement>()
const isArchived = computed(() => route.path.startsWith('/messages/archived'))

const directConversation = ref<IMessageConversation>()
const selectedConversation = computed(() => {
  if (directConversation.value?.id === threadId.value) return directConversation.value
  return conversations.value.find((item) => item.id === threadId.value)
})
const thread = useMessageThread(threadId, chatScrollRef, isArchived, selectedConversation)

const sendAddress = computed(() => {
  return resolveConversationSendAddress(
    selectedConversation.value,
    thread.items.value.map((item) => item.address),
    participantFieldsSupported.value === false
  )
})

type ConversationLookupMeta = { threadId: string; enhanced: boolean }

function isParticipantSchemaError(error: string): boolean {
  const value = error.toLowerCase()
  return value.includes('addresses') && (value.includes('field') || value.includes('validation'))
}

function handleConversationLookup(data: { smsConversations: IMessageConversation[] }, error: string, context?: QueryResponseContext) {
  const meta = context?.meta as ConversationLookupMeta | undefined
  if (!meta || meta.threadId !== threadId.value) return
  if (error) {
    if (meta.enhanced && isParticipantSchemaError(error)) {
      participantFieldsSupported.value = false
      void legacyConversationLookup.fetch(context?.variables, {
        force: true,
        latest: true,
        meta: { ...meta, enhanced: false },
      })
    }
    return
  }
  if (meta.enhanced) participantFieldsSupported.value = true
  directConversation.value = data?.smsConversations?.[0]
}

const enhancedConversationLookup = initLazyQuery({
  handle: handleConversationLookup,
  document: smsConversationsWithAddressesGQL,
})
const legacyConversationLookup = initLazyQuery({
  handle: handleConversationLookup,
  document: smsConversationsGQL,
})

function loadConversationMetadata(tid: string) {
  directConversation.value = undefined
  if (!tid) return
  const current = conversations.value.find((item) => item.id === tid)
  if (current && (current.addresses !== undefined || participantFieldsSupported.value === false)) return
  const enhanced = participantFieldsSupported.value !== false
  const request = enhanced ? enhancedConversationLookup : legacyConversationLookup
  void request.fetch(
    {
      offset: 0,
      limit: 1,
      query: buildQuery([{ name: 'thread_id', op: '', value: tid }]),
    },
    {
      force: true,
      latest: true,
      meta: { threadId: tid, enhanced } satisfies ConversationLookupMeta,
    }
  )
}

const send = useMessageSend(
  () => app.value.appDir,
  () => threadId.value,
  () => sendAddress.value,
  {
    onSmsPending: (body, address) => thread.setPendingSms(body, address),
    onSmsSent: (clientId) => {
      thread.startPendingSmsDeadline(clientId)
      thread.refetchWithRetry(clientId)
    },
    onSmsFailed: (clientId) => {
      thread.failPending(clientId)
    },
    onMmsSent: (id, body, address, attachments) => {
      thread.setPendingMms(id, body, address, attachments)
      thread.fetch()
    },
  }
)

const captureEligible = computed(() => Boolean(threadId.value && sendAddress.value && app.value.appDir))
let captureTarget: ChatCaptureTarget | null = null
let captureTargetPromise: Promise<ChatCaptureTarget> | null = null
let captureActivation = 0
let captureDisposed = false

function currentCaptureDestination(): ChatCaptureDestination {
  return snapshotMessageCaptureDestination({
    chatId: sendAddress.value,
    channelId: threadId.value,
    appDir: app.value.appDir,
  })
}

function showCaptureError() {
  toast(t('failed'), 'error')
}

function showCaptureRequestError(context: string, error: unknown) {
  void import('@/lib/screen-capture/tauri-capture-adapter')
    .then((adapter) => adapter.reportTauriCaptureError(context, error))
    .catch(() => console.error(context, error))
  showCaptureError()
}

async function consumeCapturedMms(file: File, destination: ChatCaptureDestination): Promise<void> {
  return sendCapturedMms(file, destination, {
    currentDestination: currentCaptureDestination,
    messageBody: send.messageBody,
    pendingFiles: send.pendingFiles,
    sendMessage: send.sendMessage,
  })
}

async function loadCaptureTarget(): Promise<ChatCaptureTarget> {
  if (!__IS_TAURI__) throw new Error('screen capture is available only in the desktop app')
  if (!captureTargetPromise) {
    captureTargetPromise = import('@/lib/screen-capture/tauri-capture-adapter')
      .then(async (adapter) => {
        const client = await adapter.getTauriCaptureClient(() => showCaptureError())
        const target = adapter.createChatCaptureTarget(client, consumeCapturedMms)
        captureTarget = target
        if (captureDisposed) target.dispose()
        return target
      })
      .catch((error) => {
        captureTargetPromise = null
        throw error
      })
  }
  return captureTargetPromise
}

async function activateCaptureTarget(activation: number) {
  try {
    const target = await loadCaptureTarget()
    if (captureDisposed || !isActive.value || !captureEligible.value || activation !== captureActivation) return
    target.activate(currentCaptureDestination())
  } catch (error) {
    if (!captureDisposed && isActive.value && captureEligible.value && activation === captureActivation) showCaptureRequestError('SMS capture target activation failed', error)
  }
}

async function handleCaptureRequest() {
  const activation = captureActivation
  try {
    const target = await loadCaptureTarget()
    if (captureDisposed || !isActive.value || !captureEligible.value || send.sendDisabled.value || activation !== captureActivation) return
    await target.start(currentCaptureDestination())
  } catch (error) {
    if (!captureDisposed && isActive.value && captureEligible.value && activation === captureActivation) showCaptureRequestError('SMS capture request failed', error)
  }
}

thread.setTerminalHandlers({
  onSmsFailure: (failed) => send.restoreDraft(failed.body),
  onMmsResult: (pendingId, success) => send.settleMms(pendingId, success),
})

async function onSend() {
  await send.sendMessage()
}

async function archiveConversation() {
  if (await smsStore.archiveConversations([threadId.value])) backToList()
}

function openExport() {
  openModal(ExportSmsModal, {
    items: [...thread.sortedItems.value],
    contactName: thread.contactName.value,
    urlTokenKey: urlTokenKey.value,
  })
}

function backToList() {
  const q = route.query.q
  replacePath(mainStore, q ? `/messages?q=${q}` : `/messages`)
}

const isActive = ref(false)

function applyRouteQuery(force = false) {
  const tid = route.params.threadId
  const resolved = typeof tid === 'string' ? tid : Array.isArray(tid) ? tid[0] : ''
  loadConversationMetadata(resolved)
  thread.applyThread(resolved, force)
  drainQueuedResults()
}

watch(
  () => route.fullPath,
  () => {
    if (isActive.value) applyRouteQuery(true)
  }
)

watch([threadId, sendAddress, () => app.value.appDir], () => {
  captureActivation += 1
  captureTarget?.deactivate()
  if (__IS_TAURI__ && isActive.value && captureEligible.value) void activateCaptureTarget(captureActivation)
})

let unsubscribeSmsResults: (() => void) | undefined
let unsubscribeMmsResults: (() => void) | undefined

onActivated(() => {
  isActive.value = true
  unsubscribeSmsResults = subscribeSmsSendResults(onSmsSendResult)
  unsubscribeMmsResults = subscribeMmsSendResults(onMmsSendResult)
  thread.subscribe(true)
  emitter.on('mms_sent', onMmsSent)
  applyRouteQuery(true)
  captureActivation += 1
  if (__IS_TAURI__ && captureEligible.value) void activateCaptureTarget(captureActivation)
})

onDeactivated(() => {
  isActive.value = false
  thread.unsubscribe()
  emitter.off('mms_sent', onMmsSent)
  unsubscribeSmsResults?.()
  unsubscribeMmsResults?.()
  unsubscribeSmsResults = undefined
  unsubscribeMmsResults = undefined
  captureActivation += 1
  captureTarget?.deactivate()
})

onUnmounted(() => {
  captureDisposed = true
  captureActivation += 1
  captureTarget?.dispose()
  void captureTargetPromise?.then((target) => target.dispose()).catch(() => {})
})

function onSmsSendResult(result: ISmsSendResultEvent): boolean {
  const outcome = thread.handleSmsSendResult(result)
  if (outcome.failed) send.restoreDraft(outcome.failed.body)
  return outcome.handled
}

function onMmsSendResult(result: IMmsSendResultEvent): boolean {
  const outcome = thread.handleMmsSendResult(result)
  if (outcome.handled) send.settleMms(result.pendingId, result.success)
  return outcome.handled
}

function onMmsSent(pendingId: string) {
  send.settleMms(pendingId, true)
}

function drainQueuedResults() {
  for (const pending of [...thread.pendingSmsItems.value]) {
    const result = takeSmsSendResult(pending.id)
    if (result) onSmsSendResult(result)
  }
  for (const pending of [...thread.pendingMmsItems.value]) {
    const result = takeMmsSendResult(pending.id)
    if (result) onMmsSendResult(result)
  }
}
</script>

<style lang="scss">
.page-content .main-messages {
  flex-direction: row;

  .content {
    flex: 1;
    overflow: hidden;
    width: 0px;
    display: flex;
    flex-direction: column;

    .top-app-bar {
      padding-inline: 0;
    }
  }
}
</style>
