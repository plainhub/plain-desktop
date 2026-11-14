import { invoke, onNativeReload } from '@/lib/api/native-invoke'
import { isLocalMode } from '@/lib/device/local-mode'
import { getRemoteClientId } from '@/lib/device/client-id'
import { nativeViewScope, type NativeTarget } from './native-client'

export type NativeSocketKind = 'screen' | 'imageEditor'
type SocketEvent = 'video' | 'audio' | 'imageEditorReady' | 'imageEditorUpdate'
const EVENTS: Record<number, SocketEvent> = { 1: 'video', 2: 'audio', 3: 'imageEditorReady', 4: 'imageEditorUpdate' }
export interface NativeSocketConnection { send(payload: Uint8Array): boolean; close(): void }
interface SocketCallbacks { opened?(): void; message(kind: SocketEvent, payload: Uint8Array): void; dropped(error: unknown): void }
const MAX_FRAME_BYTES = 16 * 1024 * 1024
const MAX_CONTROL_BYTES = 65536 - 48
const MAX_QUEUED_WRITES = 8

export function openNativeSocket(kind: NativeSocketKind, callbacks: SocketCallbacks): NativeSocketConnection {
  const target: NativeTarget = isLocalMode() ? { kind: 'local' } : { kind: 'peer', peerId: getRemoteClientId() }
  let active = true
  let ready = false
  let handle: string | undefined
  let scope: string | undefined
  let writes = Promise.resolve()
  let queued = 0
  const connection: NativeSocketConnection = {
    send(payload) {
      if (!active || !ready || kind !== 'screen') return false
      if (!payload.length || payload.length > MAX_CONTROL_BYTES || queued >= MAX_QUEUED_WRITES) {
        fail(new Error('Native socket command backpressure'))
        return false
      }
      const bytes = payload.slice()
      queued++
      writes = writes.then(async () => {
        if (!active) return
        await invoke<void>('native_socket_send', bytes, { headers: { 'x-plain-native-scope': scope!, 'x-plain-socket-handle': handle! } })
      }).catch(fail).finally(() => { queued-- })
      return true
    },
    close() {
      if (!active) return
      stopReload()
      active = false
      ready = false
      if (handle !== undefined) void invoke('native_socket_close', { scope, handle }).catch(console.error)
    },
  }
  const stopReload = onNativeReload(connection.close)
  function fail(error: unknown) {
    if (!active) return
    connection.close()
    callbacks.dropped(error)
  }
  void (async () => {
    if (target.kind === 'peer' && !target.peerId) throw new Error('native_peer_id_missing')
    scope = await nativeViewScope()
    if (!active) return
    handle = await invoke<string>('native_socket_open', { scope, target, kind })
    if (!active) { await invoke('native_socket_close', { scope, handle }); return }
    while (active) {
      const buffer = await invoke<ArrayBuffer>('native_socket_read', { scope, handle })
      if (!active) return
      if (!(buffer instanceof ArrayBuffer) || !buffer.byteLength || buffer.byteLength > MAX_FRAME_BYTES) throw new Error('Invalid native socket frame')
      const frame = new Uint8Array(buffer)
      if (!ready) {
        if (frame.length !== 1 || frame[0] !== 0) throw new Error('Invalid native socket readiness')
        ready = true
        callbacks.opened?.()
      } else {
        if (!(kind === 'screen' ? frame[0] === 1 || frame[0] === 2 : frame[0] === 3 && frame.length === 1 || frame[0] === 4)) throw new Error('Invalid native socket event')
        callbacks.message(EVENTS[frame[0]!]!, frame.subarray(1))
      }
    }
  })().catch(fail)
  return connection
}
