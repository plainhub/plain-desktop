import { BrowserSocket } from '@/lib/api/browser-socket'
import { openNativeSocket, type NativeSocketConnection } from '@/lib/api/native-socket'
import { getCurrentAuthToken } from '@/lib/device/current'

interface ScreenMirrorCallbacks {
  onConnected(): void
  onDisconnected(): void
  onVideo(data: Uint8Array): void
  onAudio(data: Uint8Array): void
}

export class ScreenMirrorSocket {
  private socket: BrowserSocket | null = null
  private native: NativeSocketConnection | null = null

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
    try {
      this.socket = new BrowserSocket('/screen', token, {
        connected: () => this.callbacks.onConnected(),
        disconnected: () => {
          this.socket = null
          this.callbacks.onDisconnected()
        },
        events: {
          SCREEN_MIRROR_VIDEO: data => this.callbacks.onVideo(data),
          SCREEN_MIRROR_AUDIO: data => this.callbacks.onAudio(data),
        },
      })
    } catch (error) {
      console.error(error)
      this.callbacks.onDisconnected()
    }
  }

  sendBytes(payload: Uint8Array): boolean {
    if (this.native) return this.native.send(payload)
    return this.socket?.send(payload) ?? false
  }

  sendJson(payload: unknown): boolean {
    return this.sendBytes(new TextEncoder().encode(JSON.stringify(payload)))
  }

  disconnect() {
    this.native?.close()
    this.native = null
    const socket = this.socket
    this.socket = null
    socket?.close()
  }
}
