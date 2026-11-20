// use upload queue instead of calling upload directly
import { showActionError } from '@/hooks/show-action-error'
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
        // Local previews are not valid wire references; wait for every file hash.
        await Promise.all(uploads.map((upload) => addUploadTaskAndWait(upload, false)))

        const c = item.content
        if (c.__typename !== 'ChatMessageContent') throw new Error('Attachment content required')
        const items = c.items.map(({ type, attachment: metadata }, index) => {
          const upload = uploads[index]
          if (!upload?.fileHash) throw new Error(`Upload completed without a file hash: ${upload?.file.name ?? index}`)
          return { type, attachment: {
            id: upload.id,
            uri: 'fid:' + upload.fileHash,
            size: metadata.size,
            durationMs: metadata.durationMs,
            width: metadata.width,
            height: metadata.height,
            summary: metadata.summary,
            fileName: upload.file.name,
          } }
        })

        const res = await gqlFetch(sendChatItemsGQL, { targets: [toId], content: { message: { text: c.text, format: c.format, items } } })
        const sent = res?.data?.sendChatItems?.[0]
        if (!sent) throw new Error('sendChatItems returned no message')
        try {
          onSent?.(sent)
        } catch (error) {
          // The server transaction already succeeded; a rendering error is not a send failure.
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
  const { mutate: deleteItems } = initMutation({ document: deleteChatItemsGQL }, showActionError)
  await deleteItems({ query: chatId })
}
