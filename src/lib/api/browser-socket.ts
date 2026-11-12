import { WebSocketCrypto } from './websocket-crypto'
import { parseWebSocketData } from './sjcl-arraybuffer'
import { getWebSocketBaseUrl } from './api'
import { openSocket } from './http'
import { tokenToKey } from './file'
import { getRemoteClientId, getDesktopClientId } from '@/lib/device/client-id'

interface BrowserSocketCallbacks {
  connected?(): void
  disconnected(): void
  events: Record<string, (data: Uint8Array) => void>
}

function notify(callback: () => void) {
  try { callback() } catch (error) { console.error(error) }
}

export class BrowserSocket {
  private socket: WebSocket
  private crypto: WebSocketCrypto | null = null
  private state: 'handshaking' | 'ready' | 'closing' | 'closed' = 'handshaking'
  private deadline: ReturnType<typeof setTimeout>

  constructor(path: '/screen' | '/image_editor', token: string, callbacks: BrowserSocketCallbacks) {
    const key = tokenToKey(token)
    const clientId = getDesktopClientId()
    const url = new URL(path, getWebSocketBaseUrl())
    url.searchParams.set('cid', clientId)
    const socket = this.socket = openSocket(url.toString(), getRemoteClientId())
    socket.binaryType = 'arraybuffer'
    this.deadline = setTimeout(() => this.fail('handshake_timeout'), 10000)
    socket.onopen = () => {
      if (this.state !== 'handshaking') return
      try {
        this.crypto = new WebSocketCrypto(clientId, key, path)
        socket.send(this.crypto.registration)
      } catch (error) {
        console.error(error)
        this.fail('invalid_handshake')
      }
    }
    socket.onmessage = (message: MessageEvent) => {
      if (this.state === 'closing' || this.state === 'closed') return
      let deliver: () => void
      try {
        if (!this.crypto || !(message.data instanceof ArrayBuffer)) throw new Error('Invalid WebSocket frame')
        if (this.state === 'handshaking') {
          this.crypto.acknowledge(new Uint8Array(message.data))
          socket.send(this.crypto.encode(new Uint8Array()))
          this.state = 'ready'
          clearTimeout(this.deadline)
          deliver = () => callbacks.connected?.()
        } else {
          const packet = parseWebSocketData(this.crypto.decode(new Uint8Array(message.data)))
          if (!Object.hasOwn(callbacks.events, packet.type)) throw new Error('Invalid WebSocket event')
          deliver = () => callbacks.events[packet.type]!(packet.data)
        }
      } catch (error) {
        console.error(error)
        this.fail('invalid_event')
        return
      }
      notify(deliver)
    }
    socket.onclose = () => {
      if (this.state === 'closed') return
      this.state = 'closed'
      this.crypto = null
      clearTimeout(this.deadline)
      notify(() => callbacks.disconnected())
    }
    socket.onerror = () => {
      if (this.state === 'closing' || this.state === 'closed') return
      this.state = 'closing'
      this.crypto = null
      clearTimeout(this.deadline)
      socket.close()
    }
  }

  private fail(reason: string) {
    if (this.state === 'closing' || this.state === 'closed') return
    this.state = 'closing'
    this.crypto = null
    clearTimeout(this.deadline)
    this.socket.close(1008, reason)
  }

  send(payload: Uint8Array): boolean {
    if (this.state !== 'ready' || !this.crypto || this.socket.readyState !== WebSocket.OPEN) return false
    try {
      this.socket.send(this.crypto.encode(payload))
      return true
    } catch (error) {
      console.error(error)
      this.fail('invalid_control')
      return false
    }
  }

  close() {
    if (this.state === 'closed') return
    this.state = 'closed'
    this.crypto = null
    clearTimeout(this.deadline)
    this.socket.close()
  }
}
