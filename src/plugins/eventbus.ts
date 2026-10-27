import type { IItemTagsUpdatedEvent, IItemsTagsUpdatedEvent, IFileDeletedEvent, IFileRenamedEvent, IMediaItemsActionedEvent, INotesActionedEvent } from '@/lib/interfaces'
import type { HttpServerStarted } from '@/lib/api/local-server'
import type { PairingRequest, PairingResult } from '@/lib/pairing-types'
import type { IUploadItem } from '@/stores/temp'

type Events = {
  HTTP_SERVER_STARTED: HttpServerStarted
  upload_task_done: IUploadItem
  upload_progress: IUploadItem
  refetch_app: undefined
  refetch_favorite_folders: undefined
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
  color_mode_changed: undefined
  app_socket_connection_changed: boolean
  screen_socket_connection_changed: boolean
  SCREEN_MIRROR_VIDEO: Uint8Array
  SCREEN_MIRROR_AUDIO: Uint8Array
  CHANNEL_INVITE_RECEIVED: { channelId: string, channelName: string, fromId: string, fromName: string }
  sms_sent: undefined
  PAIRING_REQUEST_RECEIVED: PairingRequest
  PAIRING_SUCCESS: PairingResult
  PAIRING_FAILED: PairingResult
  PAIRING_CANCELED: PairingResult
  PAIRING_STARTED: PairingResult
  IMAGE_EDITOR_UPDATE: ArrayBuffer
  peer_notifications_updated: import('@/lib/peer/local-peer-data').PeerNotificationUpdate
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
