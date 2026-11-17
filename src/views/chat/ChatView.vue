<template>
  <Teleport v-if="isActive" to="#header-end-slot" defer>
    <v-icon-button v-tooltip="$t('refresh')" @click="() => refetch()">
      <i-material-symbols:refresh-rounded />
    </v-icon-button>
    <v-icon-button v-tooltip="$t('files')" @click.prevent="openFolder">
      <i-lucide:folder />
    </v-icon-button>
  </Teleport>
  <div class="top-app-bar">
    <div class="title">{{ pageTitle }}</div>
  </div>
  <div ref="scrollContainer" class="chat-view-body" @scroll="onBodyScroll">
    <div v-if="loading && chatItems.length === 0" class="loading-state">
      <v-circular-progress indeterminate class="sm" />
    </div>
    <template v-else>
      <ChatMessageItem
        v-for="(chatItem, index) of visibleItems"
        :key="chatItem.id"
        :data="chatItem"
        :show-date="dateVisible(chatItem, index)"
        :sender-name="getSenderName(chatItem)"
        :sending-status="sendingText(chatItem.id)"
        :delete-loading="deleteLoading"
        :download-info="downloadProgress[chatItem.id] ?? null"
        :peer="peer"
        @delete="deleteMessage"
        @forward="handleForward"
        @retry="handleRetry"
        @download-action="handleDownloadAction"
      />
    </template>
  </div>
  <div v-if="notAllowChat" class="chat-notice">{{ noticeText }}</div>
  <ChatInput v-else :key="chatId" v-model="chatText" :create-loading="sending || sendLoading" :attachments="activeDraft.items" :format="activeDraft.format" @send-message="handleSend" @stage-attachments="stageAttachments" @remove-attachment="removeAttachment" @toggle-format="toggleFormat" @request-capture="handleCaptureRequest" />
</template>

<script setup lang="ts">
import { ref, computed, nextTick, watch, onActivated, onDeactivated, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useChatDraft } from './hooks/chat-draft'
import { formatDate } from '@/lib/format'
import ChatInput from './ChatInput.vue'
import ChatMessageItem from './ChatMessageItem.vue'
import ForwardMessageModal from './ForwardMessageModal.vue'
import { openModal } from '@/components/modal'
import { useChatRouteId } from './hooks/chat-route'
import { useChatData } from './hooks/chat-data'
import { useChatMessages } from './hooks/chat-messages'
import { useChatUpload } from './hooks/chat-upload'
import type { ChatTextFormat, DraftAttachment } from '@/lib/chat/message-draft'
import type { IChatItem } from '@/lib/interfaces'
import { ChannelStatus, PeerStatus } from '@/lib/status'
import toast from '@/components/toaster'
import type { ChatCaptureTarget } from '@/lib/screen-capture/tauri-capture-adapter'

const { t } = useI18n()
const { chatId, peerId, channelId, isChannel, appDir, openFolder } = useChatRouteId()
const { peer, channel, pageTitle, getSenderName } = useChatData(chatId, peerId, isChannel, channelId)

const notAllowChat = computed(() => {
  if (isChannel.value) {
    return !!channel.value && channel.value.status !== ChannelStatus.JOINED
  }
  return peer.value?.status === PeerStatus.UNPAIRED
})

const noticeText = computed(() => {
  if (!isChannel.value && peer.value?.status === PeerStatus.UNPAIRED) {
    return t('unpaired')
  }
  if (channel.value?.status === ChannelStatus.KICKED) {
    return t('channel_kicked_notice')
  }
  return t('channel_left_notice')
})

const chatText = useChatDraft(chatId)

const { chatItems, loading, sendLoading, deleteLoading, scrollContainer, scrollBottom, send, retryMessage, deleteMessage, refetch } = useChatMessages(chatId, channelId)

const { uploadMessage, sendingText, downloadProgress, handleDownloadAction } = useChatUpload(chatId, channelId, appDir, scrollBottom, chatItems)
type Draft = { items: DraftAttachment[]; format: ChatTextFormat }
const plainDraft: Draft = { items: [], format: 'PLAIN' }
const drafts = ref(new Map<string, Draft>())
const activeDraft = computed(() => drafts.value.get(chatId.value) ?? plainDraft)
const sending = ref(false)
function stageAttachments(items: DraftAttachment[], target = chatId.value) {
  if (!items.length) return
  const draft = drafts.value.get(target) ?? plainDraft
  drafts.value.set(target, { ...draft, items: [...draft.items, ...items] })
}
function removeAttachment(index: number) {
  const draft = activeDraft.value
  drafts.value.set(chatId.value, { ...draft, items: draft.items.filter((_, i) => i !== index) })
}
function toggleFormat() {
  const draft = activeDraft.value
  drafts.value.set(chatId.value, { ...draft, format: draft.format === 'PLAIN' ? 'MARKDOWN' : 'PLAIN' })
}

const isActive = ref(false)
let captureTarget: ChatCaptureTarget | null = null
let captureTargetPromise: Promise<ChatCaptureTarget> | null = null
let captureActivation = 0
let captureDisposed = false

function currentCaptureDestination() {
  return { chatId: chatId.value, channelId: channelId.value, appDir }
}

function showCaptureError(error: unknown) {
  void import('@/views/screen-capture/capture-error-presentation')
    .then((presentation) => presentation.presentCaptureError(error, t))
    .catch(() => toast(t('FAILED'), 'error'))
}

function showCaptureRequestError(context: string, error: unknown) {
  void import('@/lib/screen-capture/tauri-capture-adapter')
    .then((adapter) => adapter.reportTauriCaptureError(context, error))
    .catch(() => console.error(context, error))
  showCaptureError(error)
}

async function loadCaptureTarget(): Promise<ChatCaptureTarget> {
  if (!__IS_TAURI__) throw new Error('screen capture is available only in the desktop app')
  if (!captureTargetPromise) {
    captureTargetPromise = import('@/lib/screen-capture/tauri-capture-adapter')
      .then(async (adapter) => {
        const client = await adapter.getTauriCaptureClient((error) => showCaptureError(error))
        const target = adapter.createChatCaptureTarget(client, async (file, destination) => stageAttachments([{ type: 'IMAGE', file }], destination.chatId))
        captureTarget = target
        if (captureDisposed) target.dispose()
        return target
      })
      .catch((error) => {
        // A transient listener/import failure must be retryable without
        // reloading the whole chat webview.
        captureTargetPromise = null
        throw error
      })
  }
  return captureTargetPromise
}

async function activateCaptureTarget(activation: number) {
  try {
    const target = await loadCaptureTarget()
    if (captureDisposed || !isActive.value || notAllowChat.value || activation !== captureActivation) return
    target.activate(currentCaptureDestination())
  } catch (error) {
    if (!captureDisposed && isActive.value && !notAllowChat.value && activation === captureActivation) showCaptureRequestError('chat capture target activation failed', error)
  }
}

async function handleCaptureRequest() {
  const activation = captureActivation
  try {
    const target = await loadCaptureTarget()
    if (captureDisposed || !isActive.value || notAllowChat.value || activation !== captureActivation) return
    await target.start(currentCaptureDestination())
  } catch (error) {
    if (!captureDisposed && isActive.value && !notAllowChat.value && activation === captureActivation) showCaptureRequestError('chat capture request failed', error)
  }
}

// Render window: mount only the most recent slice of the conversation and
// extend it when the user scrolls to the top. Long conversations would
// otherwise mount hundreds of heavy message components at once.
const INITIAL_VISIBLE = 60
const VISIBLE_BATCH = 60
const visibleCount = ref(INITIAL_VISIBLE)
const visibleItems = computed(() =>
  chatItems.value.length > visibleCount.value ? chatItems.value.slice(-visibleCount.value) : chatItems.value,
)

function onBodyScroll() {
  const div = scrollContainer.value
  if (!div) return
  if (visibleCount.value < chatItems.value.length && div.scrollTop < 80) {
    const prevHeight = div.scrollHeight
    visibleCount.value += VISIBLE_BATCH
    nextTick(() => { if (div) div.scrollTop += div.scrollHeight - prevHeight })
  }
}

function dateVisible(item: IChatItem, index: number): boolean {
  if (index === 0) return true
  const prev = visibleItems.value[index - 1]
  return prev != null && formatDate(prev.createdAt) !== formatDate(item.createdAt)
}

async function handleSend() {
  const textDraft = chatText.snapshot()
  const text = textDraft.text
  const draft = activeDraft.value
  if (sending.value || sendLoading.value || (!text.trim() && !draft.items.length)) return
  const destination = currentCaptureDestination()
  const uploaded = draft.items.length > 0
  sending.value = true
  try {
    const success = uploaded
      ? (await uploadMessage(draft.items, text, draft.format, destination), true)
      : await send(text, draft.format)
    if (!success) return
    const remaining = drafts.value.get(destination.chatId)
    if (remaining === draft) drafts.value.delete(destination.chatId)
    else if (remaining && uploaded) {
      drafts.value.set(destination.chatId, { ...remaining, items: remaining.items.filter(item => !draft.items.includes(item)) })
    }
    chatText.clearSent(textDraft)
  } catch (error) {
    console.error('Chat message failed', error)
    toast(t('send_failed'), 'error')
  } finally {
    sending.value = false
  }
}

function handleRetry(id: string) {
  retryMessage(id)
}

function handleForward(item: IChatItem) {
  openModal(ForwardMessageModal, { message: item, excludeChatId: chatId.value })
}

onActivated(() => {
  isActive.value = true
  captureActivation += 1
  if (__IS_TAURI__ && !notAllowChat.value) void activateCaptureTarget(captureActivation)
})
onDeactivated(() => {
  isActive.value = false
  captureActivation += 1
  captureTarget?.deactivate()
})
watch([chatId, channelId, notAllowChat], () => {
  captureActivation += 1
  captureTarget?.deactivate()
  if (__IS_TAURI__ && isActive.value && !notAllowChat.value) {
    void activateCaptureTarget(captureActivation)
  }
})
watch(chatId, () => {
  visibleCount.value = INITIAL_VISIBLE
})
onUnmounted(() => {
  captureDisposed = true
  captureActivation += 1
  captureTarget?.dispose()
  void captureTargetPromise?.then((target) => target.dispose()).catch(() => {})
})
</script>

<style lang="scss">
.page-content .main-chat {
  display: flex;
  flex-direction: column;
}
</style>

<style lang="scss" scoped>
.chat-view-body {
  flex: 1;
  overflow-y: auto;
}

.loading-state {
  display: flex;
  justify-content: center;
  padding: 40px;
}

.chat-notice {
  padding: 12px 16px;
  text-align: center;
  font-size: 14px;
  color: var(--md-sys-color-on-surface);
}
</style>
