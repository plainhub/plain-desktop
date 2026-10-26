import { getWebSocketBaseUrl, getLocalToken } from '@/lib/api/api'
import { getCurrentAuthToken } from '@/lib/device/current'
import { getRemoteClientId } from '@/lib/device/client-id'
import { isLocalMode } from '@/lib/device/local-mode'
import { get as prefsGet } from '@/lib/prefs'
import { openSocket } from '@/lib/api/http'
import { tokenToKey } from '@/lib/api/file'
import { chachaEncrypt, bitArrayToUint8Array } from '@/lib/api/crypto'
import { EventStream } from '@/lib/api/websocket-events'

interface ScreenMirrorCallbacks {
  onConnected(): void
  onDisconnected(): void
  onVideo(data: Uint8Array): void
  onAudio(data: Uint8Array): void
}

export class ScreenMirrorSocket {
  private socket: WebSocket | null = null

  constructor(private callbacks: ScreenMirrorCallbacks) {}

  connect() {
    if (this.socket) return
    const local = isLocalMode()
    const token = local ? getLocalToken() : getCurrentAuthToken()
    if (!token) {
      this.callbacks.onDisconnected()
      return
    }
    const key = tokenToKey(token)
    const url = new URL('/screen', getWebSocketBaseUrl())
    url.searchParams.set('cid', prefsGet('client_id', ''))
    let socket: WebSocket
    try {
      socket = openSocket(url.toString(), local ? '' : getRemoteClientId())
    } catch (error) {
      console.error(error)
      this.callbacks.onDisconnected()
      return
    }
    this.socket = socket
    socket.binaryType = 'arraybuffer'
    let stream: EventStream | null = null
    socket.onopen = () => {
      if (this.socket !== socket) return
      const registration = bitArrayToUint8Array(chachaEncrypt(key, Date.now().toString()))
      stream = new EventStream(registration)
      socket.send(registration)
      this.callbacks.onConnected()
    }
    socket.onmessage = (message: MessageEvent) => {
      if (this.socket !== socket) return
      let packet: { type: string; data: Uint8Array }
      try {
        if (!stream || !(message.data instanceof ArrayBuffer)) throw new Error('Invalid screen frame')
        packet = stream.decode(message.data, key)
        if (!['SCREEN_MIRROR_VIDEO', 'SCREEN_MIRROR_AUDIO'].includes(packet.type)) {
          throw new Error('Invalid screen event')
        }
      } catch (error) {
        console.error(error)
        stream = null
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
      this.socket = null
      this.callbacks.onDisconnected()
    }
    socket.onerror = () => socket.close()
  }

  disconnect() {
    const socket = this.socket
    this.socket = null
    socket?.close()
  }
}
