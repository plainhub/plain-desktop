import type { IItemTagsUpdatedEvent, IItemsTagsUpdatedEvent, IFileDeletedEvent, IFileRenamedEvent, IMediaItemsActionedEvent, INotesActionedEvent, ISmsChangedEvent, ISmsSendResultEvent, IMmsSendResultEvent, IUploadMergeResultEvent, IScanProgress } from '@/lib/interfaces'
import type { PairingRequest, PairingResult } from '@/lib/pairing-types'
import type { IUploadItem } from '@/stores/temp'
import type { ScreenMirrorVideoCodec } from '@/views/screen-mirror/screen-mirror-pipeline'

type Events = {
  upload_task_done: IUploadItem
  upload_progress: IUploadItem
  UPLOAD_MERGE_RESULT: IUploadMergeResultEvent
  refetch_app: undefined
  refetch_favorite_folders: undefined
  PERMISSIONS_UPDATED: undefined
  do_play_audio: undefined
  pause_audio: undefined
  item_tags_updated: IItemTagsUpdatedEvent
  items_tags_updated: IItemsTagsUpdatedEvent
  refetch_tags: string
  media_items_actioned: IMediaItemsActionedEvent
  feed_entries_deleted: undefined
  calls_deleted: undefined
  notes_actioned: INotesActionedEvent
  file_deleted: IFileDeletedEvent
  file_renamed: IFileRenamedEvent
  toast: string
  tap_phone: string
  FEEDS_FETCHED: any
  MESSAGE_CREATED: any
  MESSAGE_UPDATED: any
  MESSAGE_DELETED: any
  NOTIFICATION_CREATED: any
  NOTIFICATION_UPDATED: any
  NOTIFICATION_DELETED: any
  NOTIFICATION_REFRESHED: any
  color_mode_changed: undefined
  app_socket_connection_changed: boolean
  POMODORO_ACTION: any
  POMODORO_SETTINGS_UPDATE: any
  SCREEN_MIRRORING: string
  SCREEN_MIRROR_VIDEO: Uint8Array
  SCREEN_MIRROR_AUDIO: Uint8Array
  SCREEN_MIRROR_VIDEO_CODEC: ScreenMirrorVideoCodec
  SCREEN_MIRROR_AUDIO_GRANTED: boolean
  BOOKMARK_UPDATED: any
  DOWNLOAD_PROGRESS: any[]
  CHANNELS_UPDATED: any[]
  PEER_STATUS_UPDATED: { id: string, online: boolean }
  CHANNEL_INVITE_RECEIVED: { channelId: string, channelName: string, fromId: string, fromName: string }
  DEVICE_NAME_UPDATED: string
  sms_sent: undefined
  IMAGE_SEARCH_UPDATED: any
  MMS_SENT: string
  SMS_PROVIDER_CHANGED: ISmsChangedEvent | null
  SMS_SEND_RESULT: ISmsSendResultEvent
  MMS_SEND_RESULT: IMmsSendResultEvent
  PAIRING_REQUEST_RECEIVED: PairingRequest
  PAIRING_SUCCESS: PairingResult
  PAIRING_FAILED: PairingResult
  PAIRING_CANCELED: PairingResult
  PAIRING_STARTED: PairingResult
  NEARBY_DEVICE_FOUND: any
  NEARBY_DEVICE_UNREACHABLE: { id: string }
  NEARBY_DISCOVERY_STARTED: undefined
  NEARBY_DISCOVERY_STOPPED: { reason?: string } | undefined
  IMAGE_EDITOR_UPDATE: ArrayBuffer
  peer_ws_event: { peerId: string, type: string, data: any }
  MEDIA_SCAN_PROGRESS: IScanProgress
  FILE_TASK_PROGRESS: any
  DLNA_RENDERER_FOUND: any
  DLNA_DISCOVERY_DONE: any
}

type Handler<T = any> = (event: T) => void

function createEmitter<E extends Record<string, any>>() {
  const all = new Map<keyof E, Set<Handler>>()

  function on<K extends keyof E>(type: K, handler: Handler<E[K]>) {
    const s = all.get(type)
    if (s) s.add(handler)
    else all.set(type, new Set([handler]))
  }

  function off<K extends keyof E>(type: K, handler?: Handler<E[K]>) {
    if (handler) all.get(type)?.delete(handler)
    else all.delete(type)
  }

   
   
  function emit<K extends keyof E>(type: K, event: E[K]): void
  // eslint-disable-next-line no-redeclare -- TypeScript function overloads share a name by design
  function emit<K extends keyof E>(type: undefined extends E[K] ? K : never): void
  // eslint-disable-next-line no-redeclare -- TypeScript function overloads share a name by design
  function emit(type: any, event?: any) {
    all.get(type)?.forEach((h) => h(event))
  }

  return { on, off, emit }
}

const emitter = createEmitter<Events>()

export default emitter
