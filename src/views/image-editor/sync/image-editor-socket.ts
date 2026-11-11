import { WebSocketCrypto } from '@/lib/api/websocket-crypto'
import { parseWebSocketData } from '@/lib/api/sjcl-arraybuffer'
import { openNativeSocket, type NativeSocketConnection } from '@/lib/api/native-socket'
import { getWebSocketBaseUrl } from '@/lib/api/api'
import { tokenToKey } from '@/lib/api/file'
import { openSocket } from '@/lib/api/http'
import { getRemoteClientId, getDesktopClientId } from '@/lib/device/client-id'
import { getCurrentAuthToken } from '@/lib/device/current'

export class ImageEditorSocket {
  private socket: WebSocket | null = null
  private handshakeDeadline: ReturnType<typeof setTimeout> | undefined
  private native: NativeSocketConnection | null = null
  private retry: ReturnType<typeof setTimeout> | undefined
  private stopped = true

  constructor(private onReady: () => void, private onUpdate: (data: Uint8Array) => void) {}

  connect() {
    this.stopped = false
    this.open()
  }

  private open() {
    if (this.stopped || this.socket || this.native) return
    if (__IS_TAURI__) {
      const native = openNativeSocket('imageEditor', {
        message: (kind, payload) => {
          if (this.native !== native) return
          if (kind === 'imageEditorReady') this.onReady()
          else this.onUpdate(payload)
        },
        dropped: error => {
          if (this.native !== native) return
          console.error(error)
          this.native = null
          this.scheduleRetry()
        },
      })
      this.native = native
      return
    }
    const token = getCurrentAuthToken()
    if (!token) return
    const key = tokenToKey(token)
    const url = new URL('/image_editor', getWebSocketBaseUrl())
    const clientId = getDesktopClientId()
    url.searchParams.set('cid', clientId)
    let socket: WebSocket
    try {
      socket = openSocket(url.toString(), getRemoteClientId())
    } catch (error) {
      console.error(error)
      this.scheduleRetry()
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
        crypto = new WebSocketCrypto(clientId, key, '/image_editor')
        socket.send(crypto.registration)
      } catch (error) {
        console.error(error)
        socket.close(1008, 'invalid_handshake')
      }
    }
    socket.onmessage = (message: MessageEvent) => {
      if (this.socket !== socket) return
      try {
        if (!crypto || !(message.data instanceof ArrayBuffer)) throw new Error('Invalid image editor frame')
        if (!acknowledged) {
          crypto.acknowledge(new Uint8Array(message.data))
          socket.send(crypto.encode(new Uint8Array()))
          acknowledged = true
          clearTimeout(this.handshakeDeadline)
          return
        }
        const packet = parseWebSocketData(crypto.decode(new Uint8Array(message.data)))
        if (packet.type === 'IMAGE_EDITOR_READY') this.onReady()
        else if (packet.type === 'IMAGE_EDITOR_UPDATE') this.onUpdate(packet.data)
        else throw new Error('Invalid image editor event')
      } catch (error) {
        console.error(error)
        crypto = null
        socket.close(1008, 'invalid_event')
      }
    }
    socket.onclose = () => {
      if (this.socket !== socket) return
      clearTimeout(this.handshakeDeadline)
      this.socket = null
      this.scheduleRetry()
    }
    socket.onerror = () => socket.close()
  }

  private scheduleRetry() {
    if (!this.stopped) this.retry = setTimeout(() => this.open(), 1000)
  }

  disconnect() {
    clearTimeout(this.handshakeDeadline)
    this.stopped = true
    this.native?.close()
    this.native = null
    clearTimeout(this.retry)
    const socket = this.socket
    this.socket = null
    socket?.close()
  }
}
