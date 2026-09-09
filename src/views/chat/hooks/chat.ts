// use upload queue instead of calling upload directly
import { addUploadTaskAndWait, removeUpload } from '@/lib/upload/upload-queue'
import { deleteChatItemsGQL, sendChatItemGQL, initMutation } from '@/lib/api/mutation'
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
        // native PNG be acknowledged until every upload and sendChatItem finish.
        await Promise.all(uploads.map((upload) => addUploadTaskAndWait(upload, false)))

        const c = item._content
        const items = c.value.items.map((metadata: any, index: number) => {
          const upload = uploads[index]
          if (!upload?.fileHash) throw new Error(`Upload completed without a file hash: ${upload?.file.name ?? index}`)
          return {
            uri: 'fid:' + upload.fileHash,
            size: metadata.size,
            duration: metadata.duration,
            width: metadata.width,
            height: metadata.height,
            summary: metadata.summary,
            fileName: upload.file.name,
          }
        })

        const res = await gqlFetch(sendChatItemGQL, { toId, content: JSON.stringify({ type: c.type, value: { items } }) })
        const sent = res?.data?.sendChatItem
        const rawItems: any[] = Array.isArray(sent) ? sent : sent ? [sent] : []
        if (!rawItems.length) throw new Error('sendChatItem returned no message')
        try {
          onSent?.(rawItems[0])
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
