import * as Y from 'yjs'
import emitter from '@/plugins/eventbus'
import { gqlFetch } from '@/lib/api/gql-client'
import { arrayBufferToBase64 } from '@/lib/strutil'
import type { SyncTransport, Unsubscribe, UpdateHandler } from './sync-transport'

const BROADCAST_DEBOUNCE_MS = 50
const IMAGE_EDITOR_UPDATE_EVENT = 'image_editor_update'

function decodeFrame(data: ArrayBuffer): { pid: string; update: Uint8Array } | null {
  const bytes = new Uint8Array(data)
  if (bytes.length < 1) return null
  const pidLen = bytes[0]!
  if (bytes.length < 1 + pidLen) return null
  const pid = new TextDecoder().decode(bytes.subarray(1, 1 + pidLen))
  const update = bytes.subarray(1 + pidLen)
  return { pid, update }
}

export class EventSyncTransport implements SyncTransport {
  private _connected = false
  private _handlers = new Set<UpdateHandler>()
  private _getProjectId: () => string | null
  private _broadcastTimer: ReturnType<typeof setTimeout> | null = null
  private _pendingUpdates: Uint8Array[] = []
  private _eventHandler: ((data: ArrayBuffer) => void) | null = null

  constructor(getProjectId: () => string | null) {
    this._getProjectId = getProjectId
  }

  get connected(): boolean { return this._connected }

  async connect(): Promise<void> {
    if (this._connected) return
    this._eventHandler = (data: ArrayBuffer) => {
      const frame = decodeFrame(data)
      if (!frame) return
      if (this._getProjectId() !== frame.pid) return
      if (frame.update.length > 0) {
        this._handlers.forEach(h => h(frame.update))
      }
    }
    emitter.on(IMAGE_EDITOR_UPDATE_EVENT, this._eventHandler)
    this._connected = true
  }

  disconnect(): void {
    if (this._eventHandler) {
      emitter.off(IMAGE_EDITOR_UPDATE_EVENT, this._eventHandler)
      this._eventHandler = null
    }
    if (this._broadcastTimer) {
      clearTimeout(this._broadcastTimer)
      this._broadcastTimer = null
    }
    this._pendingUpdates = []
    this._connected = false
  }

  broadcastUpdate(update: Uint8Array): void {
    this._pendingUpdates.push(update)
    if (this._broadcastTimer) return
    this._broadcastTimer = setTimeout(() => this._flushBroadcast(), BROADCAST_DEBOUNCE_MS)
  }

  private async _flushBroadcast(): Promise<void> {
    this._broadcastTimer = null
    const pid = this._getProjectId()
    if (!pid || this._pendingUpdates.length === 0) return
    const merged = this._pendingUpdates.length === 1
      ? this._pendingUpdates[0]!
      : Y.mergeUpdates(this._pendingUpdates)
    this._pendingUpdates = []
    const b64 = arrayBufferToBase64(merged.buffer)
    try {
      await gqlFetch(
        `mutation BroadcastImageEditorUpdate($id: ID!, $update: String!) {
          broadcastImageEditorUpdate(id: $id, update: $update)
        }`,
        { id: pid, update: b64 },
      )
    } catch (e) {
      console.warn('[ImageEditor] Failed to broadcast update', e)
    }
  }

  onUpdate(handler: UpdateHandler): Unsubscribe {
    this._handlers.add(handler)
    return () => { this._handlers.delete(handler) }
  }

  destroy(): void {
    this.disconnect()
    this._handlers.clear()
  }
}
