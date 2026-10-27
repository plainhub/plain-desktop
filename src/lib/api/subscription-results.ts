import type { IChatItem, INotification, IScanProgress } from '@/lib/interfaces'
import type { Bookmark } from '@/stores/bookmarks'

type ChatSubscriptionItem = Pick<IChatItem, 'id' | 'fromId' | 'toId' | 'channelId' | 'content' | 'createdAt'> & {
  updatedAt: string
  status: NonNullable<IChatItem['status']>
  statusData: string
}

export interface SubscriptionResults {
  peerClipboardChanged: { peerId: string }
  deviceNameUpdated: string
  permissionsUpdated: boolean
  contentChanged: boolean
  screenMirrorState: { running: boolean }
  screenMirrorVideoCodec: { annexB: string } | null
  screenMirrorAudioGranted: boolean
  notificationCreated: Omit<INotification, 'icon'>
  notificationUpdated: Omit<INotification, 'icon'>
  notificationDeleted: Omit<INotification, 'icon'>
  notificationRefreshed: boolean
  messagesCreated: ChatSubscriptionItem[]
  messagesUpdated: ChatSubscriptionItem[]
  messagesDeleted: string
  mediaLibraryUpdated: { types: number[] }
  mediaScanProgress: IScanProgress
  bookmarksUpdated: Bookmark[]
  feedsFetched: { feedId: string; error?: string | null; cancelled?: boolean | null }
  uploadMergeResult: { fileId: string; ok: boolean; value: string | null; mergedSize: number | null; error: string | null }
  smsProviderChanged: { uris: string[] } | null
  smsSendResult: { requestId: string | null; success: boolean; resultCode: number | null }
  mmsSendResult: { pendingId: string; success: boolean; resultCode: number | null }
  mmsSent: string
  clipboardChanged: { text: string; sensitive: boolean; time: number }
}
export type SubscriptionField = keyof SubscriptionResults

export type SmsSendResult = SubscriptionResults['smsSendResult']
export type MmsSendResult = SubscriptionResults['mmsSendResult']
