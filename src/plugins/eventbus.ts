import type { IItemTagsUpdatedEvent, IItemsTagsUpdatedEvent, IFileDeletedEvent, IFileRenamedEvent, IMediaItemsActionedEvent, INotesActionedEvent, ISmsChangedEvent, ISmsSendResultEvent, IMmsSendResultEvent } from '@/lib/interfaces'
import type { PairingRequest, PairingResult } from '@/lib/pairing-types'
import type { IUploadItem } from '@/stores/temp'
import type { ScreenMirrorVideoCodec } from '@/views/screen-mirror/screen-mirror-pipeline'

type Events = {
  upload_task_done: IUploadItem
  upload_progress: IUploadItem
  refetch_app: undefined
  play_audio: undefined
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
  feeds_fetched: any
  message_created: any
  message_updated: any
  message_deleted: any
  notification_created: any
  notification_updated: any
  notification_deleted: any
  notification_refreshed: any
  color_mode_changed: undefined
  app_socket_connection_changed: boolean
  pomodoro_action: any
  pomodoro_settings_update: any
  screen_mirroring: string
  screen_mirror_video: Uint8Array
  screen_mirror_audio: Uint8Array
  screen_mirror_video_codec: ScreenMirrorVideoCodec
  screen_mirror_audio_granted: boolean
  bookmark_updated: any
  download_progress: any[]
  channels_updated: any[]
  peer_status_updated: { id: string, online: boolean }
  channel_invite_received: { channelId: string, channelName: string, fromId: string, fromName: string }
  device_name_updated: string
  sms_sent: undefined
  image_search_updated: any
  mms_sent: string
  sms_changed: ISmsChangedEvent | null
  sms_send_result: ISmsSendResultEvent
  mms_send_result: IMmsSendResultEvent
  pairing_request_received: PairingRequest
  pairing_success: PairingResult
  pairing_failed: PairingResult
  pairing_canceled: PairingResult
  pairing_started: PairingResult
  nearby_device_found: any
  nearby_discovery_started: undefined
  nearby_discovery_stopped: { reason?: string } | undefined
  image_editor_update: ArrayBuffer
  peer_ws_event: { peerId: string, type: number, data: any }
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
