import { invoke } from '@tauri-apps/api/core'
import { getWebSocketBaseUrl, getLocalToken, isNativeTarget } from './api'
import { getCurrentAuthToken } from '@/lib/device/current'
import { getRemoteClientId } from '@/lib/device/client-id'
import { isLocalMode } from '@/lib/device/local-mode'
import { get as prefsGet } from '@/lib/prefs'
import { tokenToKey } from './file'
import { openSocket } from './http'
import { GraphqlSubscriptionCrypto } from './graphql-subscription-crypto'

export interface SubscriptionConnection { send(message: unknown): void; close(): void }
interface Callbacks { opened(): void; message(message: unknown): void; dropped(error: unknown): void }
export function openSubscriptionConnection(callbacks: Callbacks): SubscriptionConnection {
  return isNativeTarget() ? openNative(callbacks) : openNetwork(callbacks)
}
function openNetwork(callbacks: Callbacks): SubscriptionConnection {
  const url = new URL('/graphql', getWebSocketBaseUrl())
  const client = prefsGet('client_id', '')
  const token = isLocalMode() ? getLocalToken() : getCurrentAuthToken()
  if (!token) throw new Error('Subscription session missing')
  url.searchParams.set('cid', client)
  const socket = openSocket(url.toString(), isLocalMode() ? '' : getRemoteClientId(), 'plain-graphql-ws-v1')
  socket.binaryType = 'arraybuffer'
  let active = true
  let acknowledged = false
  let crypto: GraphqlSubscriptionCrypto | undefined
  let heartbeat: ReturnType<typeof setInterval> | undefined
  let pong: ReturnType<typeof setTimeout> | undefined
  const deadline = setTimeout(() => fail(new Error('Subscription handshake timeout')), 10000)
  const connection: SubscriptionConnection = {
    send(message) {
      try {
        if (!active || !acknowledged || !crypto) throw new Error('Subscription connection unavailable')
        socket.send(crypto.encode(message))
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
    if (socket.protocol !== 'plain-graphql-ws-v1') { fail(new Error('Subscription protocol mismatch')); return }
    try {
      crypto = new GraphqlSubscriptionCrypto(client, tokenToKey(token))
      socket.send(crypto.registration)
    } catch (error) { fail(error) }
  }
  socket.onmessage = (event: MessageEvent) => {
    if (!active) return
    try {
      if (!crypto || !(event.data instanceof ArrayBuffer)) throw new Error('Encrypted subscription frame required')
      if (!acknowledged) {
        crypto.acknowledge(new Uint8Array(event.data))
        acknowledged = true
        clearTimeout(deadline)
        callbacks.opened()
        heartbeat = setInterval(() => {
          connection.send({ type: 'ping' })
          pong = setTimeout(() => fail(new Error('Subscription heartbeat timeout')), 10000)
        }, 20000)
      } else {
        const message = crypto.decode(new Uint8Array(event.data))
        if (typeof message === 'object' && message !== null && 'type' in message && message.type === 'pong') clearTimeout(pong)
        else callbacks.message(message)
      }
    } catch (error) { fail(error) }
  }
  socket.onerror = () => fail(new Error('Subscription connection error'))
  socket.onclose = () => fail(new Error('Subscription connection closed'))
  return connection
}
function openNative(callbacks: Callbacks): SubscriptionConnection {
  let active = true
  let handle: number | undefined
  let writes = Promise.resolve()
  const connection: SubscriptionConnection = {
    send(message) {
      if (!active || handle === undefined) throw new Error('Native subscription unavailable')
      const target = handle
      writes = writes.then(() => active ? invoke<void>('native_subscription_send', { handle: target, message: JSON.stringify(message) }) : undefined).catch(fail)
    },
    close() {
      active = false
      if (handle !== undefined) void invoke('native_subscription_close', { handle }).catch(console.error)
      handle = undefined
    },
  }
  function fail(error: unknown) { if (active) { connection.close(); callbacks.dropped(error) } }
  void (async () => {
    const opened = await invoke<number>('native_subscription_open')
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
