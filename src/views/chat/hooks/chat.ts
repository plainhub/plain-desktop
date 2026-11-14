// use upload queue instead of calling upload directly
import { addUploadTaskAndWait, removeUpload } from '@/lib/upload/upload-queue'
import { deleteChatItemsGQL, sendChatItemsGQL, initMutation } from '@/lib/api/mutation'
import type { IChatItem } from '@/lib/interfaces'
import type { IUploadItem } from '@/stores/temp'
import { gqlFetch } from '@/lib/api/gql-client'

interface IChatTask {
  uploads: IUploadItem[]
  item: IChatItem
  toId: string
  onSent?: (sentItem: any) => void
}

// All task consumers within one webview must address the same registry. Chat
// message deletion and sidebar clearing call useTasks() independently from the
// upload hook, but still need to cancel the transaction that owns the File.
const activeTasks: Map<string, IChatTask> = new Map()

export const useTasks = () => {
  const cancelTask = (task: IChatTask) => {
    for (const upload of task.uploads) removeUpload(upload.id)
    activeTasks.delete(task.item.id)
  }

  return {
    async enqueue(item: IChatItem, uploads: IUploadItem[], toId: string, onSent?: (sentItem: any) => void) {
      const task: IChatTask = { item, uploads, toId, onSent }
      activeTasks.set(item.id, task)
      try {
        // The returned promise is the capture delivery boundary: do not let the
        // native PNG be acknowledged until every upload and sendChatItems finish.
        await Promise.all(uploads.map((upload) => addUploadTaskAndWait(upload, false)))

        const c = item.content
        if (c.__typename !== 'ChatImagesContent' && c.__typename !== 'ChatFilesContent') throw new Error('Attachment content required')
        const items = c.items.map((metadata: any, index: number) => {
          const upload = uploads[index]
          if (!upload?.fileHash) throw new Error(`Upload completed without a file hash: ${upload?.file.name ?? index}`)
          return {
            id: upload.id,
            uri: 'fid:' + upload.fileHash,
            size: metadata.size,
            durationMs: metadata.durationMs,
            width: metadata.width,
            height: metadata.height,
            summary: metadata.summary,
            fileName: upload.file.name,
          }
        })

        const res = await gqlFetch(sendChatItemsGQL, { targets: [toId], content: c.__typename === 'ChatImagesContent' ? { images: { items } } : { files: { items } } })
        const sent = res?.data?.sendChatItems?.[0]
        if (!sent) throw new Error('sendChatItems returned no message')
        try {
          onSent?.(sent)
        } catch (error) {
          // The server transaction already succeeded. A local rendering error
          // must not release and redeliver the same native capture.
          console.error('Failed to display the sent upload message', error)
        }
      } catch (error) {
        cancelTask(task)
        throw error
      } finally {
        activeTasks.delete(item.id)
      }
    },
    cancel(messageId: string) {
      const task = activeTasks.get(messageId)
      if (task) cancelTask(task)
    },
    cancelByChatId(chatId: string) {
      for (const task of activeTasks.values()) {
        if (task.toId === chatId) cancelTask(task)
      }
    },
  }
}

export async function clearChatMessages(chatId: string, cancelByChatId: (chatId: string) => void) {
  cancelByChatId(chatId)
  const { mutate: deleteItems } = initMutation({ document: deleteChatItemsGQL })
  await deleteItems({ query: chatId })
}
