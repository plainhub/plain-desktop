import { isLocalMode } from '@/lib/device/local-mode'
import { invoke, onNativeReload } from '@/lib/api/native-invoke'
import { getWebSocketBaseUrl } from './api'
import { getCurrentAuthToken } from '@/lib/device/current'
import { getRemoteClientId, getDesktopClientId } from '@/lib/device/client-id'
import { tokenToKey } from './file'
import { openSocket } from './http'
import { WebSocketCrypto } from './websocket-crypto'
import { peerProxyPath } from '../url'
import { nativeViewScope, type NativeTarget } from './native-client'

export interface SubscriptionConnection { send(message: unknown): void; close(): void }
export interface SubscriptionCallbacks { opened(): void; message(message: unknown): void; dropped(error: unknown): void }
export function openSubscriptionConnection(callbacks: SubscriptionCallbacks, peerId?: string): SubscriptionConnection {
  if (!__IS_TAURI__) return openNetwork(callbacks, peerId)
  if (peerId !== undefined) throw new Error('Native peer-proxy subscriptions are unsupported')
  return openNative(callbacks, isLocalMode() ? { kind: 'local' } : { kind: 'peer', peerId: getRemoteClientId() })
}
function openNetwork(callbacks: SubscriptionCallbacks, peerId?: string): SubscriptionConnection {
  const url = new URL(peerId ? peerProxyPath(peerId, 'graphql') : '/graphql', getWebSocketBaseUrl())
  const client = getDesktopClientId()
  const token = getCurrentAuthToken()
  if (!token) throw new Error('Subscription session missing')
  url.searchParams.set('cid', client)
  const socket = openSocket(url.toString(), getRemoteClientId())
  socket.binaryType = 'arraybuffer'
  let active = true
  let acknowledged = false
  let crypto: WebSocketCrypto | undefined
  let heartbeat: ReturnType<typeof setInterval> | undefined
  let pong: ReturnType<typeof setTimeout> | undefined
  const deadline = setTimeout(() => fail(new Error('Subscription handshake timeout')), 10000)
  const connection: SubscriptionConnection = {
    send(message) {
      try {
        if (!active || !acknowledged || !crypto) throw new Error('Subscription connection unavailable')
        socket.send(crypto.encode(new TextEncoder().encode(JSON.stringify(message))))
      } catch (error) { fail(error) }
    },
    close() {
      active = false
      crypto = undefined
      clearTimeout(deadline)
      clearTimeout(pong)
      clearInterval(heartbeat)
      socket.close()
    },
  }
  function fail(error: unknown) {
    if (!active) return
    connection.close()
    callbacks.dropped(error)
  }
  socket.onopen = () => {
    if (!active) return
    try {
      crypto = new WebSocketCrypto(client, tokenToKey(token))
      socket.send(crypto.registration)
    } catch (error) { fail(error) }
  }
  socket.onmessage = (event: MessageEvent) => {
    if (!active) return
    try {
      if (!crypto || !(event.data instanceof ArrayBuffer)) throw new Error('Encrypted subscription frame required')
      if (!acknowledged) {
        crypto.acknowledge(new Uint8Array(event.data))
        socket.send(crypto.encode(new Uint8Array()))
        acknowledged = true
        clearTimeout(deadline)
        if (!active) return
        callbacks.opened()
        if (!active) return
        heartbeat = setInterval(() => {
          connection.send({ type: 'ping' })
          if (!active) return
          pong = setTimeout(() => fail(new Error('Subscription heartbeat timeout')), 10000)
        }, 20000)
      } else {
        const message = JSON.parse(new TextDecoder().decode(crypto.decode(new Uint8Array(event.data))))
        if (typeof message === 'object' && message !== null && 'type' in message && message.type === 'pong') clearTimeout(pong)
        else callbacks.message(message)
      }
    } catch (error) { fail(error) }
  }
  socket.onerror = () => fail(new Error('Subscription connection error'))
  socket.onclose = () => fail(new Error('Subscription connection closed'))
  return connection
}
export function openNative(callbacks: SubscriptionCallbacks, target: NativeTarget = { kind: 'local' }): SubscriptionConnection {
  if (target.kind === 'peer' && !target.peerId) throw new Error('native_peer_id_missing')
  let active = true
  let handle: number | undefined
  let writes = Promise.resolve()
  const connection: SubscriptionConnection = {
    send(message) {
      if (!active || handle === undefined) throw new Error('Native subscription unavailable')
      const opened = handle
      writes = writes.then(() => active ? invoke<void>('native_subscription_send', { handle: opened, message: JSON.stringify(message) }) : undefined).catch(fail)
    },
    close() {
      stopReload()
      active = false
      if (handle !== undefined) void invoke('native_subscription_close', { handle }).catch(console.error)
      handle = undefined
    },
  }
  const stopReload = onNativeReload(connection.close)
  function fail(error: unknown) { if (active) { connection.close(); callbacks.dropped(error) } }
  void (async () => {
    const scope = await nativeViewScope()
    if (!active) return
    const opened = await invoke<number>('native_subscription_open', { scope, target })
    if (!active) { await invoke('native_subscription_close', { handle: opened }); return }
    handle = opened
    callbacks.opened()
    while (active) {
      const message = await invoke<string>('native_subscription_read', { handle: opened })
      if (active) callbacks.message(JSON.parse(message))
    }
  })().catch(fail)
  return connection
}
