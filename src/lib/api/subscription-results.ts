import type { IChatItem, INotification, IFileIndexScanProgress, IImageSearchStatus } from '@/lib/interfaces'
import type { GqlResultType } from './graphql/operations'
import type { Bookmark } from '@/stores/bookmarks'

type ChatSubscriptionItem = Pick<IChatItem, 'id' | 'fromId' | 'toId' | 'channelId' | 'content' | 'createdAt'> & {
  updatedAt: string
  status: NonNullable<IChatItem['status']>
  deliveryResults: IChatItem['deliveryResults']
}

export interface SubscriptionResults {
  pairingRequested: import('@/lib/pairing-types').PairingRequest
  pairingStarted: { deviceId: string; deviceName: string; error: string | null; verificationCode: string | null }
  pairingFailed: { deviceId: string; deviceName: string; error: string | null }
  pairingCanceled: { deviceId: string; deviceName: string; error: string | null }
  channelsChanged: {
    changed: boolean
    invite: { channelId: string; channelName: string; ownerPeerId: string; ownerPeerName: string } | null
    cancel: { channelId: string; ownerPeerId: string } | null
  }
  peersChanged: boolean
  pairingCompleted: { deviceId: string; deviceName: string; ip: string; port?: number | null; deviceType?: import('@/lib/status').DeviceType | null; requestId?: string | null }
  downloadsChanged: boolean
  prefsChanged: boolean
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
  deviceNameUpdated: string
  accessUpdated: boolean
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
  mediaUpdated: GqlResultType<'mediaUpdatedSubscription'>['mediaUpdated']
  fileIndexScanProgressUpdated: IFileIndexScanProgress
  bookmarksUpdated: Bookmark[]
  feedsFetched: { feedId: string; error?: string | null; canceled?: boolean | null }
  uploadMergeResult: { fileId: string; ok: boolean; value: string | null; mergedSize: number | null; error: string | null }
  smsChanged: { uris: string[] } | null
  smsSendResult: { requestId: string; success: boolean; resultCode: number | null }
  clipboardChanged: { text: string; sensitive: boolean; time: string }
}
export type SubscriptionField = keyof SubscriptionResults

export type SmsSendResult = SubscriptionResults['smsSendResult']
