import type { AccessFeature } from '@/lib/interfaces'
import type { SubscriptionField } from './subscription-results'

export interface SubscriptionOperation {
  field: SubscriptionField
  document: string
  access: readonly AccessFeature[]
  capability?: string
}
export const subscriptionOperations: readonly SubscriptionOperation[] = [
  {field: 'pairingRequested', document: 'subscription { pairingRequested { fromId fromName fromIp deviceType verificationCode identityChanged } }', access: []},
  {field: 'pairingStarted', document: 'subscription { pairingStarted { deviceId deviceName error verificationCode } }', access: []},
  {field: 'pairingFailed', document: 'subscription { pairingFailed { deviceId deviceName error } }', access: []},
  {field: 'pairingCanceled', document: 'subscription { pairingCanceled { deviceId deviceName error } }', access: []},
  {field: 'channelsChanged', document: 'subscription { channelsChanged { changed invite { channelId channelName ownerPeerId ownerPeerName } cancel { channelId ownerPeerId } } }', access: []},
  {field: 'peersChanged', document: 'subscription { peersChanged }', access: []},
  {field: 'pairingCompleted', document: 'subscription { pairingCompleted { deviceId deviceName ip port deviceType requestId } }', access: []},
  {field: 'downloadsChanged', document: 'subscription { downloadsChanged }', access: []},
  {field: 'prefsChanged', document: 'subscription { prefsChanged }', access: []},
  {field: 'pomodoroAction', document: 'subscription { pomodoroAction { today { date state timeLeftSec totalTimeSec completedCount currentRound isRunning isPaused } completedState } }', access: []},
  {field: 'pomodoroSettingsChanged', document: 'subscription { pomodoroSettingsChanged }', access: []},
  {field: 'imageModelsUpdated', document: 'subscription { imageModelsUpdated { version status downloadProgress errorMessage modelSize modelDir isIndexing totalImages indexedImages } }', access: []},
  {"field": "deviceNameUpdated", "document": "subscription { deviceNameUpdated }", "access": []},
  {"field": "accessUpdated", "document": "subscription { accessUpdated }", "access": []},
  {"field": "contentChanged", "document": "subscription { contentChanged }", "access": []},
  {"field": "screenMirrorState", "document": "subscription { screenMirrorState { running } }", "access": [], "capability": "SCREEN_MIRROR"},
  {"field": "screenMirrorVideoCodec", "document": "subscription { screenMirrorVideoCodec { annexB } }", "access": [], "capability": "SCREEN_MIRROR"},
  {"field": "screenMirrorAudioGranted", "document": "subscription { screenMirrorAudioGranted }", "access": [], "capability": "SCREEN_MIRROR"},
  {"field": "notificationCreated", "document": "subscription { notificationCreated { id onlyOnce isClearable appId appName postedAt silent title body actions replyActions } }", "access": ["NOTIFICATIONS"], "capability": "NOTIFICATIONS"},
  {"field": "notificationUpdated", "document": "subscription { notificationUpdated { id onlyOnce isClearable appId appName postedAt silent title body actions replyActions } }", "access": ["NOTIFICATIONS"], "capability": "NOTIFICATIONS"},
  {"field": "notificationDeleted", "document": "subscription { notificationDeleted { id onlyOnce isClearable appId appName postedAt silent title body actions replyActions } }", "access": ["NOTIFICATIONS"], "capability": "NOTIFICATIONS"},
  {"field": "notificationRefreshed", "document": "subscription { notificationRefreshed }", "access": ["NOTIFICATIONS"], "capability": "NOTIFICATIONS"},
  {"field": "messagesCreated", "document": "subscription { messagesCreated { id fromId toId channelId content { ... on ChatTextContent { __typename text linkPreviews { url title description imageUrl imageLocalPath imageWidth imageHeight siteName domain createdAt } } ... on ChatImagesContent { __typename items { id uri size durationMs width height summary fileName } } ... on ChatFilesContent { __typename items { id uri size durationMs width height summary fileName } } ... on ChatShareContent { __typename shareId urlToken peerInfo { id ip port } name itemCount totalSize expiresAt } } createdAt updatedAt status deliveryResults { peerId peerName error } } }", "access": []},
  {"field": "messagesUpdated", "document": "subscription { messagesUpdated { id fromId toId channelId content { ... on ChatTextContent { __typename text linkPreviews { url title description imageUrl imageLocalPath imageWidth imageHeight siteName domain createdAt } } ... on ChatImagesContent { __typename items { id uri size durationMs width height summary fileName } } ... on ChatFilesContent { __typename items { id uri size durationMs width height summary fileName } } ... on ChatShareContent { __typename shareId urlToken peerInfo { id ip port } name itemCount totalSize expiresAt } } createdAt updatedAt status deliveryResults { peerId peerName error } } }", "access": []},
  {"field": "messagesDeleted", "document": "subscription { messagesDeleted }", "access": []},
  {"field": "mediaUpdated", "document": "subscription { mediaUpdated { types } }", "access": ["IMAGES", "VIDEOS", "AUDIO", "DOCS", "FILES"]},
  {"field": "fileIndexScanProgressUpdated", "document": "subscription { fileIndexScanProgressUpdated { indexed pending total state error } }", "access": ["FILES"]},
  {"field": "bookmarksUpdated", "document": "subscription { bookmarksUpdated { id url title faviconPath groupId pinned clickCount lastClickedAt sortOrder createdAt updatedAt } }", "access": []},
  {"field": "feedsFetched", "document": "subscription { feedsFetched { feedId error canceled } }", "access": []},
  {"field": "uploadMergeResult", "document": "subscription { uploadMergeResult { fileId ok value mergedSize error } }", "access": ["FILES"]},
  {"field": "smsChanged", "document": "subscription { smsChanged { uris } }", "access": ["SMS"], "capability": "TELEPHONY"},
  {"field": "smsSendResult", "document": "subscription { smsSendResult { requestId success resultCode } }", "access": ["SMS"], "capability": "TELEPHONY"},
  {"field": "clipboardChanged", "document": "subscription { clipboardChanged { text sensitive time } }", "access": ["CLIPBOARD"], "capability": "CLIPBOARD"},
]
