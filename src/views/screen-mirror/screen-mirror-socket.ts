import { WebSocketCrypto } from '@/lib/api/websocket-crypto'
import { parseWebSocketData } from '@/lib/api/sjcl-arraybuffer'
import { openNativeSocket, type NativeSocketConnection } from '@/lib/api/native-socket'
import { getWebSocketBaseUrl } from '@/lib/api/api'
import { getCurrentAuthToken } from '@/lib/device/current'
import { getRemoteClientId, getDesktopClientId } from '@/lib/device/client-id'
import { openSocket } from '@/lib/api/http'
import { tokenToKey } from '@/lib/api/file'

interface ScreenMirrorCallbacks {
  onConnected(): void
  onDisconnected(): void
  onVideo(data: Uint8Array): void
  onAudio(data: Uint8Array): void
}

export class ScreenMirrorSocket {
  private socket: WebSocket | null = null
  private handshakeDeadline: ReturnType<typeof setTimeout> | undefined
  private native: NativeSocketConnection | null = null
  private crypto: WebSocketCrypto | null = null

  constructor(private callbacks: ScreenMirrorCallbacks) {}

  connect() {
    if (this.socket || this.native) return
    if (__IS_TAURI__) {
      const native = openNativeSocket('screen', {
        opened: () => { if (this.native === native) this.callbacks.onConnected() },
        message: (kind, payload) => {
          if (this.native !== native) return
          try {
            if (kind === 'video') this.callbacks.onVideo(payload)
            else this.callbacks.onAudio(payload)
          } catch (error) { console.error(error) }
        },
        dropped: error => {
          if (this.native !== native) return
          console.error(error)
          this.native = null
          this.callbacks.onDisconnected()
        },
      })
      this.native = native
      return
    }
    const token = getCurrentAuthToken()
    if (!token) {
      this.callbacks.onDisconnected()
      return
    }
    const key = tokenToKey(token)
    const url = new URL('/screen', getWebSocketBaseUrl())
    const clientId = getDesktopClientId()
    url.searchParams.set('cid', clientId)
    let socket: WebSocket
    try {
      socket = openSocket(url.toString(), getRemoteClientId())
    } catch (error) {
      console.error(error)
      this.callbacks.onDisconnected()
      return
    }
    this.socket = socket
    socket.binaryType = 'arraybuffer'
    let crypto: WebSocketCrypto | null = null
    let acknowledged = false
    this.handshakeDeadline = setTimeout(() => {
      if (this.socket !== socket) return
      crypto = null
      socket.close(1008, 'handshake_timeout')
    }, 10000)
    socket.onopen = () => {
      if (this.socket !== socket) return
      try {
        crypto = new WebSocketCrypto(clientId, key, '/screen')
        socket.send(crypto.registration)
      } catch (error) {
        console.error(error)
        socket.close(1008, 'invalid_handshake')
      }
    }
    socket.onmessage = (message: MessageEvent) => {
      if (this.socket !== socket) return
      let packet: { type: string; data: Uint8Array }
      try {
        if (!crypto || !(message.data instanceof ArrayBuffer)) throw new Error('Invalid screen frame')
        if (!acknowledged) {
          crypto.acknowledge(new Uint8Array(message.data))
          socket.send(crypto.encode(new Uint8Array()))
          acknowledged = true
          clearTimeout(this.handshakeDeadline)
          this.crypto = crypto
          this.callbacks.onConnected()
          return
        }
        if (this.crypto !== crypto) return
        packet = parseWebSocketData(crypto.decode(new Uint8Array(message.data)))
        if (!['SCREEN_MIRROR_VIDEO', 'SCREEN_MIRROR_AUDIO'].includes(packet.type)) {
          throw new Error('Invalid screen event')
        }
      } catch (error) {
        console.error(error)
        crypto = null
        this.crypto = null
        socket.close(1008, 'invalid_event')
        return
      }
      try {
        if (packet.type === 'SCREEN_MIRROR_VIDEO') this.callbacks.onVideo(packet.data)
        else this.callbacks.onAudio(packet.data)
      } catch (error) {
        console.error(error)
      }
    }
    socket.onclose = () => {
      if (this.socket !== socket) return
      clearTimeout(this.handshakeDeadline)
      this.socket = null
      this.crypto = null
      this.callbacks.onDisconnected()
    }
    socket.onerror = () => socket.close()
  }

  sendBytes(payload: Uint8Array): boolean {
    if (this.native) return this.native.send(payload)
    if (!this.socket || !this.crypto || this.socket.readyState !== WebSocket.OPEN) return false
    try {
      this.socket.send(this.crypto.encode(payload))
      return true
    } catch (error) {
      console.error(error)
      this.crypto = null
      this.socket.close(1008, 'invalid_control')
      return false
    }
  }

  sendJson(payload: unknown): boolean {
    return this.sendBytes(new TextEncoder().encode(JSON.stringify(payload)))
  }

  disconnect() {
    clearTimeout(this.handshakeDeadline)
    this.native?.close()
    this.native = null
    const socket = this.socket
    this.socket = null
    this.crypto = null
    socket?.close()
  }
}
