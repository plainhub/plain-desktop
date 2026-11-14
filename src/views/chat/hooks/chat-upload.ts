import { ref, reactive, onMounted, onUnmounted, watch, type ComputedRef, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { IChatItem } from '@/lib/interfaces'
import type { IUploadItem } from '@/stores/temp'
import { useChatFilesUpload } from '@/hooks/upload'
import { useTasks } from './chat'
import { normalizeChatItem } from './chat-subscriptions'
import { generateRecordId } from '@/lib/record-id.generated'
import { MessageType } from '@/lib/status'
import { formatFileSize } from '@/lib/format'
import { getVideoData, getImageData, isVideo } from '@/lib/file'
import { gqlFetch } from '@/lib/api/gql-client'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { createDownloadGQL, pauseDownloadGQL, resumeDownloadGQL, retryDownloadGQL } from '@/lib/api/mutation'
import emitter from '@/plugins/eventbus'
import { projectChatDownloadProgress, type ChatDownloadProgress, type ChatDownloadProgressItem } from '../chat-download-progress'

export function messageNeedsDownload(item: IChatItem): boolean {
  const items = 'items' in item.content ? item.content.items : []
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
  const downloadProgress = reactive<Record<string, ChatDownloadProgress>>(Object.create(null))

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
        id: upload.id,
        fileName: upload.file.name,
        dir: frozenDestination.appDir,
        uri: upload.fileName || upload.file.name,
        size: upload.file.size,
        durationMs: 0,
        width: 0,
        height: 0,
        summary: options.summary ?? '',
      }
      if (upload.file.type.startsWith('video') || isVideo(upload.file.name)) {
        const v = await getVideoData(upload.file)
        itemProps.durationMs = v.durationMs
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

    // Optimistic local preview: images have no generated thumbnail, so point
    // one at the blob URL (rendered before the /fs id resolves server-side;
    // never sent — the wire content is rebuilt from upload hashes).
    valueItems.forEach((item: any, index: number) => {
      if (!item.thumbnail) item.thumbnail = objectUrls[index]
    })
    const content: IChatItem['content'] = { __typename: contentType === MessageType.IMAGES ? 'ChatImagesContent' : 'ChatFilesContent', items: valueItems }
    const item: IChatItem = {
      id: 'new_' + generateRecordId(),
      fromId: 'me',
      toId: frozenDestination.chatId,
      channelId: frozenDestination.channelId,
      createdAt: new Date().toISOString(),
      content,
      deliveryResults: [],
      __typename: 'ChatItem',
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
    const summaryText = message.substring(0, 250).replace(/[\uD800-\uDBFF]$/, '').trim()
    const summary = summaryText.lastIndexOf(' ') > 230 ? summaryText.substring(0, summaryText.lastIndexOf(' ')) + '...' : summaryText + '...'
    await handleContentUpload([file], MessageType.FILES, currentDestination(), { summary })
    chatText.value = ''
  }

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
      const message = chatItems.value.find((item) => item.id === messageId)
      const fileIds = (message && 'items' in message.content ? message.content.items : [])
        ?.filter((file: any) => typeof file.uri === 'string' && file.uri.startsWith('fsid:'))
        .map((file: any) => file.id)
      if (!fileIds?.length) return
      await Promise.all(fileIds.map(async (fileId: string) => {
        const result = await gqlFetch(createDownloadGQL, { input: { chat: { messageId, peerId, fileIds: [fileId] } } })
        if (result.errors?.length) throw new Error(result.errors[0].message)
      }))
      await refreshDownloads()
    } catch (e) {
      submittedDownloads.delete(messageId)
      console.error(e)
    }
  }

  async function controlDownload(messageId: string, action: 'pause' | 'resume' | 'retry') {
    const taskIds = downloadProgress[messageId]?.taskIds ?? []
    const document = action === 'pause' ? pauseDownloadGQL : action === 'resume' ? resumeDownloadGQL : retryDownloadGQL
    for (const id of taskIds) {
      const result = await gqlFetch(document, { id })
      if (result.errors?.length) throw new Error(result.errors[0].message)
    }
    await refreshDownloads()
  }

  function handleDownloadAction(messageId: string, action: 'pause' | 'resume' | 'retry') {
    void controlDownload(messageId, action).catch(console.error)
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
  let stopDownloads: (() => void) | undefined
  let stopDownloadsAttached: (() => void) | undefined
  let downloadsPending = false
  let downloadsRefreshing = false
  let downloadsActive = false

  async function refreshDownloads() {
    downloadsPending = true
    if (downloadsRefreshing) return
    downloadsRefreshing = true
    try {
      while (downloadsPending && downloadsActive) {
        downloadsPending = false
        const result = await gqlFetch<{ downloads: Array<{ id: string; downloadedBytes: number; totalBytes: number; bytesPerSecond: number; status: string; source: { __typename: string; messageId?: string } }> }>(
          'query ChatDownloadProgress { downloads { id downloadedBytes totalBytes bytesPerSecond status source { __typename ... on ChatDownloadSource { messageId } } } }',
          undefined,
          { fresh: true },
        )
        if (result.errors?.length) throw new Error(result.errors[0].message)
        if (!downloadsActive) return
        const rows: ChatDownloadProgressItem[] = result.data.downloads
          .filter((task) => task.source.__typename === 'ChatDownloadSource')
          .map((task) => ({ id: task.id, messageId: task.source.messageId!, downloaded: task.downloadedBytes, total: task.totalBytes, speed: task.bytesPerSecond, status: task.status }))
        const { active, finished } = projectChatDownloadProgress(rows, Object.keys(downloadProgress))
        for (const messageId of finished) submittedDownloads.delete(messageId)
        Object.keys(downloadProgress).forEach((key) => delete downloadProgress[key])
        Object.assign(downloadProgress, active)
      }
    } finally {
      downloadsRefreshing = false
    }
  }

  onMounted(() => {
    downloadsActive = true
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

    stopDownloads = graphqlSubscriptions.subscribe('downloadsChanged', refreshDownloads)
    stopDownloadsAttached = graphqlSubscriptions.onAttached(() => { void refreshDownloads().catch(console.error) })
    void refreshDownloads().catch(console.error)
  })

  onUnmounted(() => {
    downloadsActive = false
    stopDownloads?.()
    stopDownloadsAttached?.()
    Object.entries(handlers).forEach(([event, fn]) => emitter.off(event as any, fn))
    for (const messageId of [...activeMessageIds]) {
      cancelTask(messageId)
      cleanupMessageUpload(messageId)
    }
  })

  return { doUploadFiles, doUploadImages, sendLongMessageAsFile, sendingAgg, sendingText, downloadProgress, handleDownloadAction }
}
