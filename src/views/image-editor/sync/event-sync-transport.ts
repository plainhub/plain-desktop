import * as Y from 'yjs'
import emitter from '@/plugins/eventbus'
import { isNativeTarget } from '@/lib/api/api'
import { gqlFetch } from '@/lib/api/gql-client'
import { arrayBufferToBase64 } from '@/lib/strutil'
import { ImageEditorSocket } from './image-editor-socket'
import type { SyncTransport, Unsubscribe, UpdateHandler } from './sync-transport'

const BROADCAST_DEBOUNCE_MS = 50
const CHUNK_SIZE = 512 * 1024
const MAX_UPDATE_BYTES = 24 * 1024 * 1024
const MAGIC = [0x50, 0x49, 0x45, 0x31]
const HEADER_SIZE = 25

function decodeFrame(data: Uint8Array): { pid: string; update: Uint8Array } | null {
  if (data.length < 1) return null
  const pidLen = data[0]!
  if (data.length < 1 + pidLen) return null
  return { pid: new TextDecoder().decode(data.subarray(1, 1 + pidLen)), update: data.subarray(1 + pidLen) }
}

export class EventSyncTransport implements SyncTransport {
  private connectedState = false
  private handlers = new Set<UpdateHandler>()
  private broadcastTimer: ReturnType<typeof setTimeout> | undefined
  private syncRetry: ReturnType<typeof setTimeout> | undefined
  private pendingUpdates: Uint8Array[] = []
  private socket: ImageEditorSocket | undefined
  private instance = crypto.getRandomValues(new Uint8Array(16))
  private parts = new Map<string, { chunks: Map<number, Uint8Array>; count: number; size: number; timer: ReturnType<typeof setTimeout> }>()
  private queued = Promise.resolve()

  constructor(private getProjectId: () => string | null, private getState: () => Uint8Array) {}

  get connected(): boolean { return this.connectedState }

  async connect(): Promise<void> {
    if (this.connectedState) return
    this.connectedState = true
    if (isNativeTarget()) {
      emitter.on('IMAGE_EDITOR_UPDATE', this.onNativeUpdate)
      emitter.on('app_socket_connection_changed', this.onNativeConnection)
      this.synchronize()
    } else {
      this.socket = new ImageEditorSocket(() => this.synchronize(), data => this.receive(data))
      this.socket.connect()
    }
  }

  private onNativeUpdate = (data: ArrayBuffer | Uint8Array) => this.receive(new Uint8Array(data))
  private onNativeConnection = (connected: boolean) => { if (connected) this.synchronize() }

  private synchronize() {
    this.enqueue(new Uint8Array([0]))
    this.sendUpdate(this.getState())
  }

  resynchronize() {
    if (this.connectedState) this.synchronize()
  }

  private receive(data: Uint8Array) {
    const frame = decodeFrame(data)
    if (!frame || this.getProjectId() !== frame.pid || frame.update.length === 0) return
    const update = frame.update
    if (!MAGIC.every((value, index) => update[index] === value)) {
      this.handlers.forEach(handler => handler(update))
      return
    }
    if (update.length < HEADER_SIZE) return
    const kind = update[4]
    const id = update.subarray(5, 21)
    if (kind === 0) {
      if (!id.every((value, index) => value === this.instance[index])) this.sendUpdate(this.getState())
      return
    }
    if (kind !== 1) return
    const view = new DataView(update.buffer, update.byteOffset, update.byteLength)
    const index = view.getUint16(21)
    const count = view.getUint16(23)
    if (!count || count > Math.ceil(MAX_UPDATE_BYTES / CHUNK_SIZE) || index >= count || update.length - HEADER_SIZE > CHUNK_SIZE) return
    const key = Array.from(id, value => value.toString(16).padStart(2, '0')).join('')
    let entry = this.parts.get(key)
    if (!entry) {
      if (this.parts.size >= 8) return
      entry = { chunks: new Map(), count, size: 0, timer: setTimeout(() => {
        this.parts.delete(key)
        this.resynchronize()
      }, 30000) }
      this.parts.set(key, entry)
    }
    if (entry.count !== count || entry.chunks.has(index)) return
    const chunk = update.slice(HEADER_SIZE)
    entry.chunks.set(index, chunk)
    entry.size += chunk.length
    const buffered = [...this.parts.values()].reduce((sum, part) => sum + part.size, 0)
    if (entry.size > MAX_UPDATE_BYTES || buffered > 32 * 1024 * 1024) {
      clearTimeout(entry.timer)
      this.parts.delete(key)
      this.resynchronize()
      return
    }
    if (entry.chunks.size !== count) return
    const merged = new Uint8Array(entry.size)
    let offset = 0
    for (let i = 0; i < count; i++) {
      const part = entry.chunks.get(i)!
      merged.set(part, offset)
      offset += part.length
    }
    clearTimeout(entry.timer)
    this.parts.delete(key)
    this.handlers.forEach(handler => handler(merged))
  }

  disconnect(): void {
    this.connectedState = false
    this.socket?.disconnect()
    this.socket = undefined
    emitter.off('IMAGE_EDITOR_UPDATE', this.onNativeUpdate)
    emitter.off('app_socket_connection_changed', this.onNativeConnection)
    clearTimeout(this.broadcastTimer)
    clearTimeout(this.syncRetry)
    this.broadcastTimer = undefined
    this.syncRetry = undefined
    this.pendingUpdates = []
    for (const entry of this.parts.values()) clearTimeout(entry.timer)
    this.parts.clear()
  }

  broadcastUpdate(update: Uint8Array): void {
    this.pendingUpdates.push(update)
    if (this.broadcastTimer) return
    this.broadcastTimer = setTimeout(() => {
      this.broadcastTimer = undefined
      const updates = this.pendingUpdates
      this.pendingUpdates = []
      if (updates.length) this.sendUpdate(updates.length === 1 ? updates[0]! : Y.mergeUpdates(updates))
    }, BROADCAST_DEBOUNCE_MS)
  }

  private sendUpdate(update: Uint8Array) {
    if (update.length > MAX_UPDATE_BYTES) {
      console.error('Image editor update exceeds maximum size')
      return
    }
    const count = Math.max(1, Math.ceil(update.length / CHUNK_SIZE))
    const id = crypto.getRandomValues(new Uint8Array(16))
    for (let index = 0; index < count; index++) {
      const chunk = update.subarray(index * CHUNK_SIZE, (index + 1) * CHUNK_SIZE)
      const payload = new Uint8Array(HEADER_SIZE + chunk.length)
      payload.set(MAGIC)
      payload[4] = 1
      payload.set(id, 5)
      const header = new DataView(payload.buffer)
      header.setUint16(21, index)
      header.setUint16(23, count)
      payload.set(chunk, HEADER_SIZE)
      this.enqueue(payload)
    }
  }

  private enqueue(update: Uint8Array) {
    const id = this.getProjectId()
    if (!id || !this.connectedState) return
    if (update.length === 1 && update[0] === 0) {
      update = new Uint8Array(HEADER_SIZE)
      update.set(MAGIC)
      update.set(this.instance, 5)
    }
    this.queued = this.queued.then(async () => {
      if (!this.connectedState || this.getProjectId() !== id) return
      const result = await gqlFetch(
        `mutation BroadcastImageEditorUpdate($id: ID!, $update: String!) {
          broadcastImageEditorUpdate(id: $id, update: $update)
        }`,
        { id, update: arrayBufferToBase64(update.buffer) },
        { dedupe: false },
      )
      if (result.errors?.length) throw new Error(result.errors[0].message)
    }).catch(error => {
      console.error('Image editor update failed', error)
      if (this.connectedState && !this.syncRetry) {
        this.syncRetry = setTimeout(() => {
          this.syncRetry = undefined
          this.resynchronize()
        }, 1000)
      }
    })
  }

  onUpdate(handler: UpdateHandler): Unsubscribe {
    this.handlers.add(handler)
    return () => { this.handlers.delete(handler) }
  }

  destroy(): void {
    this.disconnect()
    this.handlers.clear()
  }
}
