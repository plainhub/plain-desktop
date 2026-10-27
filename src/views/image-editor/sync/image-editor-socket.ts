import { getLocalToken, getWebSocketBaseUrl } from '@/lib/api/api'
import { chachaEncrypt, bitArrayToUint8Array } from '@/lib/api/crypto'
import { tokenToKey } from '@/lib/api/file'
import { openSocket } from '@/lib/api/http'
import { EventStream } from '@/lib/api/websocket-events'
import { getRemoteClientId } from '@/lib/device/client-id'
import { getCurrentAuthToken } from '@/lib/device/current'
import { isLocalMode } from '@/lib/device/local-mode'
import { get as prefsGet } from '@/lib/prefs'

export class ImageEditorSocket {
  private socket: WebSocket | null = null
  private retry: ReturnType<typeof setTimeout> | undefined
  private stopped = true

  constructor(private onReady: () => void, private onUpdate: (data: Uint8Array) => void) {}

  connect() {
    this.stopped = false
    this.open()
  }

  private open() {
    if (this.stopped || this.socket) return
    const local = isLocalMode()
    const token = local ? getLocalToken() : getCurrentAuthToken()
    if (!token) return
    const key = tokenToKey(token)
    const url = new URL('/image_editor', getWebSocketBaseUrl())
    url.searchParams.set('cid', prefsGet('client_id', ''))
    let socket: WebSocket
    try {
      socket = openSocket(url.toString(), local ? '' : getRemoteClientId())
    } catch (error) {
      console.error(error)
      this.scheduleRetry()
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
    }
    socket.onmessage = (message: MessageEvent) => {
      if (this.socket !== socket) return
      try {
        if (!stream || !(message.data instanceof ArrayBuffer)) throw new Error('Invalid image editor frame')
        const packet = stream.decode(message.data, key)
        if (packet.type === 'IMAGE_EDITOR_READY') this.onReady()
        else if (packet.type === 'IMAGE_EDITOR_UPDATE') this.onUpdate(packet.data)
        else throw new Error('Invalid image editor event')
      } catch (error) {
        console.error(error)
        socket.close(1008, 'invalid_event')
      }
    }
    socket.onclose = () => {
      if (this.socket !== socket) return
      this.socket = null
      this.scheduleRetry()
    }
    socket.onerror = () => socket.close()
  }

  private scheduleRetry() {
    if (!this.stopped) this.retry = setTimeout(() => this.open(), 1000)
  }

  disconnect() {
    this.stopped = true
    clearTimeout(this.retry)
    const socket = this.socket
    this.socket = null
    socket?.close()
  }
}
