import type { SubscriptionField } from './subscription-results'

export interface SubscriptionOperation {
  field: SubscriptionField
  document: string
  permissions: readonly string[]
  capability?: string
  localOnly?: boolean
}
export const subscriptionOperations: readonly SubscriptionOperation[] = [
  {field: 'pomodoroAction', document: 'subscription { pomodoroAction { today { date state timeLeftSec totalTimeSec completedCount currentRound isRunning isPaused } completedState } }', permissions: []},
  {field: 'pomodoroSettingsChanged', document: 'subscription { pomodoroSettingsChanged }', permissions: []},
  {field: 'imageModelsUpdated', document: 'subscription { imageModelsUpdated { version status downloadProgress errorMessage modelSize modelDir isIndexing totalImages indexedImages } }', permissions: []},
  {field: 'peerClipboardChanged', document: 'subscription { peerClipboardChanged { peerId } }', permissions: [], localOnly: true},
  {"field": "deviceNameUpdated", "document": "subscription { deviceNameUpdated }", "permissions": []},
  {"field": "permissionsUpdated", "document": "subscription { permissionsUpdated }", "permissions": []},
  {"field": "contentChanged", "document": "subscription { contentChanged }", "permissions": []},
  {"field": "screenMirrorState", "document": "subscription { screenMirrorState { running } }", "permissions": [], "capability": "SCREEN_MIRROR"},
  {"field": "screenMirrorVideoCodec", "document": "subscription { screenMirrorVideoCodec { annexB } }", "permissions": [], "capability": "SCREEN_MIRROR"},
  {"field": "screenMirrorAudioGranted", "document": "subscription { screenMirrorAudioGranted }", "permissions": [], "capability": "SCREEN_MIRROR"},
  {"field": "notificationCreated", "document": "subscription { notificationCreated { id onlyOnce isClearable appId appName postedAt silent title body actions replyActions } }", "permissions": ["NOTIFICATION_LISTENER"], "capability": "NOTIFICATIONS"},
  {"field": "notificationUpdated", "document": "subscription { notificationUpdated { id onlyOnce isClearable appId appName postedAt silent title body actions replyActions } }", "permissions": ["NOTIFICATION_LISTENER"], "capability": "NOTIFICATIONS"},
  {"field": "notificationDeleted", "document": "subscription { notificationDeleted { id onlyOnce isClearable appId appName postedAt silent title body actions replyActions } }", "permissions": ["NOTIFICATION_LISTENER"], "capability": "NOTIFICATIONS"},
  {"field": "notificationRefreshed", "document": "subscription { notificationRefreshed }", "permissions": ["NOTIFICATION_LISTENER"], "capability": "NOTIFICATIONS"},
  {"field": "messagesCreated", "document": "subscription { messagesCreated { id fromId toId channelId content createdAt updatedAt status statusData } }", "permissions": []},
  {"field": "messagesUpdated", "document": "subscription { messagesUpdated { id fromId toId channelId content createdAt updatedAt status statusData } }", "permissions": []},
  {"field": "messagesDeleted", "document": "subscription { messagesDeleted }", "permissions": []},
  {"field": "mediaLibraryUpdated", "document": "subscription { mediaLibraryUpdated { types } }", "permissions": ["WRITE_EXTERNAL_STORAGE"]},
  {"field": "mediaScanProgress", "document": "subscription { mediaScanProgress { indexed pending total state error } }", "permissions": ["WRITE_EXTERNAL_STORAGE"]},
  {"field": "bookmarksUpdated", "document": "subscription { bookmarksUpdated { id url title faviconPath groupId pinned clickCount lastClickedAt sortOrder createdAt updatedAt } }", "permissions": []},
  {"field": "feedsFetched", "document": "subscription { feedsFetched { feedId error cancelled } }", "permissions": []},
  {"field": "uploadMergeResult", "document": "subscription { uploadMergeResult { fileId ok value mergedSize error } }", "permissions": ["WRITE_EXTERNAL_STORAGE"]},
  {"field": "smsProviderChanged", "document": "subscription { smsProviderChanged { uris } }", "permissions": ["READ_SMS"], "capability": "SMS"},
  {"field": "smsSendResult", "document": "subscription { smsSendResult { requestId success resultCode } }", "permissions": ["SEND_SMS"], "capability": "SMS"},
  {"field": "mmsSendResult", "document": "subscription { mmsSendResult { pendingId success resultCode } }", "permissions": ["SEND_SMS"], "capability": "SMS"},
  {"field": "mmsSent", "document": "subscription { mmsSent }", "permissions": ["SEND_SMS"], "capability": "SMS"},
  {"field": "clipboardChanged", "document": "subscription { clipboardChanged { text sensitive time } }", "permissions": ["CLIPBOARD"], "capability": "CLIPBOARD"},
]
