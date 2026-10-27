import type { IChatItem, INotification, IScanProgress, IImageSearchStatus } from '@/lib/interfaces'
import type { Bookmark } from '@/stores/bookmarks'

type ChatSubscriptionItem = Pick<IChatItem, 'id' | 'fromId' | 'toId' | 'channelId' | 'content' | 'createdAt'> & {
  updatedAt: string
  status: NonNullable<IChatItem['status']>
  statusData: string
}

export interface SubscriptionResults {
  networkChanged: boolean
  pairingChanged: boolean
  pairingRequested: import('@/lib/pairing-types').PairingRequest
  pairingStarted: { deviceId: string; deviceName: string; error: string | null }
  pairingFailed: { deviceId: string; deviceName: string; error: string | null }
  pairingCanceled: { deviceId: string; deviceName: string; error: string | null }
  channelsChanged: {
    changed: boolean
    invite: { channelId: string; channelName: string; ownerPeerId: string; ownerPeerName: string } | null
    cancel: { channelId: string; ownerPeerId: string } | null
  }
  peersChanged: boolean
  pairingCompleted: { deviceId: string; deviceName: string; ip: string }
  downloadsChanged: boolean
  preferencesChanged: boolean
  mdnsChanged: boolean
  nearbyDevicesChanged: boolean
  pomodoroAction: {
    today: {
      date: string
      state: 'WORK' | 'SHORT_BREAK' | 'LONG_BREAK'
      timeLeftSec: number
      totalTimeSec: number
      completedCount: number
      currentRound: number
      isRunning: boolean
      isPaused: boolean
    }
    completedState: 'WORK' | 'SHORT_BREAK' | 'LONG_BREAK' | null
  }
  pomodoroSettingsChanged: boolean
  imageModelsUpdated: IImageSearchStatus & { version: number }
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
