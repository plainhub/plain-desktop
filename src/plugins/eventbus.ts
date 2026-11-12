import type { IItemTagsUpdatedEvent, IItemsTagsUpdatedEvent, IFileDeletedEvent, IFileRenamedEvent, IMediaItemsActionedEvent, INotesActionedEvent } from '@/lib/interfaces'
import type { PairingRequest, PairingResult } from '@/lib/pairing-types'
import type { IUploadItem } from '@/stores/temp'

type Events = {
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
  screen_socket_connection_changed: boolean
  CHANNEL_INVITE_RECEIVED: { channelId: string, channelName: string, fromId: string, fromName: string }
  sms_sent: undefined
  PAIRING_REQUESTED: PairingRequest
  PAIRING_COMPLETED: PairingResult
  PAIRING_FAILED: PairingResult
  PAIRING_CANCELED: PairingResult
  PAIRING_STARTED: PairingResult
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
