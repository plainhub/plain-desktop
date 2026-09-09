import { ref, reactive, onMounted, onUnmounted, watch, type ComputedRef, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { IChatItem } from '@/lib/interfaces'
import type { IUploadItem } from '@/stores/temp'
import { useChatFilesUpload } from '@/hooks/upload'
import { useTasks } from './chat'
import { normalizeChatItem } from './chat-events'
import { shortUUID } from '@/lib/strutil'
import { MessageType } from '@/lib/status'
import { formatFileSize } from '@/lib/format'
import { getVideoData, getImageData, isVideo } from '@/lib/file'
import { gqlFetch } from '@/lib/api/gql-client'
import { downloadPeerFileGQL, pauseDownloadGQL, resumeDownloadGQL, retryDownloadGQL } from '@/lib/api/mutation'
import emitter from '@/plugins/eventbus'

export function messageNeedsDownload(item: IChatItem): boolean {
  const items = item._content?.value?.items
  if (!Array.isArray(items)) return false
  return items.some((f: any) => typeof f.uri === 'string' && f.uri.startsWith('fsid:'))
}

export interface ChatUploadDestination {
  readonly chatId: string
  readonly channelId: string
  readonly appDir: string
}

export function snapshotChatUploadDestination(destination: ChatUploadDestination): ChatUploadDestination {
  return Object.freeze({
    chatId: destination.chatId,
    channelId: destination.channelId,
    appDir: destination.appDir,
  })
}

export function useChatUpload(chatId: ComputedRef<string>, channelId: ComputedRef<string>, appDir: string, scrollBottom: () => void, chatText: Ref<string>, chatItems: Ref<IChatItem[]>) {
  const { t } = useI18n()
  const { getUploads } = useChatFilesUpload()
  const { enqueue: enqueueTask, cancel: cancelTask } = useTasks()

  const uploading = ref<IUploadItem[]>([])
  const messageUploads = reactive<Record<string, IUploadItem[]>>({})
  const uploadToMessage = new Map<string, string>()
  const sendingAgg = reactive<Record<string, { uploaded: number; speed: number }>>({})
  const messageObjectUrls = new Map<string, string[]>()
  const activeMessageIds = new Set<string>()
  const downloadProgress = reactive<Record<string, { downloaded: number; total: number; speed: number; status: string }>>({})

  function sendingText(messageId: string) {
    const agg = sendingAgg[messageId]
    if (!agg) return t('sending') as unknown as string
    return `${t('sending')} ${formatFileSize(agg.uploaded)} (${formatFileSize(agg.speed)}/s)`
  }

  function currentDestination(): ChatUploadDestination {
    return snapshotChatUploadDestination({ chatId: chatId.value, channelId: channelId.value, appDir })
  }

  function cleanupMessageUpload(messageId: string): void {
    const uploads = messageUploads[messageId] ?? []
    const uploadIds = new Set(uploads.map((upload) => upload.id))
    uploading.value = uploading.value.filter((upload) => !uploadIds.has(upload.id))
    for (const upload of uploads) uploadToMessage.delete(upload.id)
    delete messageUploads[messageId]
    delete sendingAgg[messageId]
    for (const url of messageObjectUrls.get(messageId) ?? []) {
      try {
        URL.revokeObjectURL(url)
      } catch (error) {
        console.error('Failed to release optimistic upload URL', error)
      }
    }
    messageObjectUrls.delete(messageId)
    activeMessageIds.delete(messageId)
  }

  async function handleContentUpload(files: File[], contentType: MessageType, destination: ChatUploadDestination, options: { summary?: string } = {}) {
    const frozenDestination = snapshotChatUploadDestination(destination)
    const uploads = getUploads(frozenDestination.appDir, files)
    uploads.forEach((u) => {
      u.status = 'pending'
      u.isAppFile = true
    })

    const valueItems: any[] = []
    for (const upload of uploads) {
      const itemProps: any = {
        dir: frozenDestination.appDir,
        uri: upload.fileName || upload.file.name,
        size: upload.file.size,
        duration: 0,
        width: 0,
        height: 0,
        summary: options.summary,
      }
      if (upload.file.type.startsWith('video') || isVideo(upload.file.name)) {
        const v = await getVideoData(upload.file)
        itemProps.duration = v.duration
        itemProps.thumbnail = v.thumbnail
        itemProps.width = v.width
        itemProps.height = v.height
      } else if (upload.file.type.startsWith('image')) {
        const v = await getImageData(upload.file)
        itemProps.width = v.width
        itemProps.height = v.height
      }
      valueItems.push(itemProps)
    }

    const objectUrls: string[] = []
    try {
      for (const upload of uploads) objectUrls.push(URL.createObjectURL(upload.file))
    } catch (error) {
      for (const url of objectUrls) {
        try {
          URL.revokeObjectURL(url)
        } catch {
          // Preserve the object-URL creation error that aborted this upload.
        }
      }
      throw error
    }

    const _content = { type: contentType, value: { items: valueItems } }
    const item: IChatItem = {
      id: 'new_' + shortUUID(),
      fromId: 'me',
      toId: frozenDestination.chatId,
      channelId: frozenDestination.channelId,
      createdAt: new Date().toISOString(),
      content: JSON.stringify(_content),
      _content,
      __typename: 'ChatItem',
      data: { ids: objectUrls },
    }

    messageObjectUrls.set(item.id, objectUrls)
    activeMessageIds.add(item.id)
    messageUploads[item.id] = uploads
    uploads.forEach((u) => uploadToMessage.set(u.id, item.id))
    sendingAgg[item.id] = {
      uploaded: uploads.reduce((s, u) => s + (u.uploadedSize || 0), 0),
      speed: uploads.reduce((s, u) => s + (u.uploadSpeed || 0), 0),
    }
    uploading.value = [...uploading.value, ...uploads]
    const tempId = item.id
    chatItems.value = [...chatItems.value, item]
    scrollBottom()
    try {
      await enqueueTask(item, uploads, frozenDestination.chatId, (sentItem) => {
        chatItems.value = chatItems.value.filter((i) => i.id !== tempId)
        const normalized = normalizeChatItem(sentItem)
        if (!chatItems.value.some((i) => i.id === normalized.id)) {
          chatItems.value = [...chatItems.value, normalized]
        }
        scrollBottom()
      })
    } catch (error) {
      chatItems.value = chatItems.value.filter((candidate) => candidate.id !== tempId)
      throw error
    } finally {
      cleanupMessageUpload(tempId)
    }
  }

  async function doUploadFiles(files: File[]) {
    if (files.length) await handleContentUpload(files, MessageType.FILES, currentDestination())
  }

  async function doUploadImages(files: File[], destination: ChatUploadDestination = currentDestination()) {
    if (files.length) await handleContentUpload(files, MessageType.IMAGES, destination)
  }

  async function sendLongMessageAsFile(message: string) {
    const file = new File([message], `message-${Date.now()}.txt`, { type: 'text/plain' })
    const summaryText = message.substring(0, 250).trim()
    const summary = summaryText.lastIndexOf(' ') > 230 ? summaryText.substring(0, summaryText.lastIndexOf(' ')) + '...' : summaryText + '...'
    await handleContentUpload([file], MessageType.FILES, currentDestination(), { summary })
    chatText.value = ''
  }

  // --- Peer file download (mirrors plain-app DownloadQueue) ---
  const submittedDownloads = new Set<string>()

  function getPeerIdForItem(item: IChatItem): string {
    if (channelId.value) return item.fromId
    const pid = chatId.value.startsWith('peer:') ? chatId.value.slice(5) : ''
    return pid === 'local' ? '' : pid
  }

  async function triggerPeerDownload(messageId: string, peerId: string) {
    if (!peerId || submittedDownloads.has(messageId)) return
    submittedDownloads.add(messageId)
    try {
      await gqlFetch(downloadPeerFileGQL, { messageId, peerId })
    } catch (e) {
      submittedDownloads.delete(messageId)
    }
  }

  async function pausePeerDownload(messageId: string) {
    try {
      await gqlFetch(pauseDownloadGQL, { messageId })
    } catch {
      /* */
    }
  }

  async function resumePeerDownload(messageId: string, peerId: string) {
    try {
      await gqlFetch(resumeDownloadGQL, { messageId, peerId })
    } catch {
      /* */
    }
  }

  async function retryPeerDownload(messageId: string, peerId: string) {
    try {
      await gqlFetch(retryDownloadGQL, { messageId, peerId })
    } catch {
      /* */
    }
  }

  function handleDownloadAction(messageId: string, action: 'pause' | 'resume' | 'retry') {
    const item = chatItems.value.find((i) => i.id === messageId)
    const peerId = item ? getPeerIdForItem(item) : ''
    if (action === 'pause') pausePeerDownload(messageId)
    else if (action === 'resume') resumePeerDownload(messageId, peerId)
    else if (action === 'retry') retryPeerDownload(messageId, peerId)
  }

  // Auto-trigger download for peer messages with fsid: URIs (mirrors
  // plain-app ChatImageItem's LaunchedEffect).
  watch(
    chatItems,
    (items) => {
      for (const item of items) {
        if (item.id.startsWith('new_')) continue
        if (downloadProgress[item.id]) continue
        if (!messageNeedsDownload(item)) continue
        const peerId = getPeerIdForItem(item)
        if (!peerId) continue
        triggerPeerDownload(item.id, peerId)
      }
    },
    { flush: 'post' }
  )

  // Event bus handlers
  const handlers: Record<string, (...args: any[]) => any> = {}

  onMounted(() => {
    handlers.upload_progress = (u: IUploadItem) => {
      const mid = uploadToMessage.get(u.id)
      if (!mid) return
      const list = messageUploads[mid]
      if (!list) return
      sendingAgg[mid] = {
        uploaded: list.reduce((s, it) => s + (it.uploadedSize || 0), 0),
        speed: list.reduce((s, it) => s + (it.uploadSpeed || 0), 0),
      }
    }
    emitter.on('upload_progress', handlers.upload_progress)

    handlers.download_progress = (items: any[]) => {
      const newProgress: Record<string, { downloaded: number; total: number; speed: number; status: string }> = {}
      for (const item of items) {
        const msgId = item.messageId
        if (!newProgress[msgId]) newProgress[msgId] = { downloaded: 0, total: 0, speed: 0, status: 'PENDING' }
        newProgress[msgId].downloaded += item.downloaded
        newProgress[msgId].total += item.total
        newProgress[msgId].speed += item.speed
        const s = item.status
        const cur = newProgress[msgId].status
        if (s === 'DOWNLOADING') newProgress[msgId].status = 'DOWNLOADING'
        else if (s === 'PAUSED' && cur !== 'DOWNLOADING') newProgress[msgId].status = 'PAUSED'
        else if (s === 'FAILED' && cur === 'PENDING') newProgress[msgId].status = 'FAILED'
        else if (s === 'COMPLETED') {
          newProgress[msgId].status = 'COMPLETED'
          submittedDownloads.delete(msgId)
        }
      }
      // Drop completed entries so the overlay disappears.
      for (const key of Object.keys(newProgress)) {
        if (newProgress[key].status === 'COMPLETED') delete newProgress[key]
      }
      Object.keys(downloadProgress).forEach((k) => delete downloadProgress[k])
      Object.assign(downloadProgress, newProgress)
    }
    emitter.on('download_progress', handlers.download_progress)
  })

  onUnmounted(() => {
    Object.entries(handlers).forEach(([event, fn]) => emitter.off(event as any, fn))
    for (const messageId of [...activeMessageIds]) {
      cancelTask(messageId)
      cleanupMessageUpload(messageId)
    }
  })

  return { doUploadFiles, doUploadImages, sendLongMessageAsFile, sendingAgg, sendingText, downloadProgress, handleDownloadAction }
}
