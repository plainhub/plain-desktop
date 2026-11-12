import { BrowserSocket } from '@/lib/api/browser-socket'
import { openNativeSocket, type NativeSocketConnection } from '@/lib/api/native-socket'
import { getCurrentAuthToken } from '@/lib/device/current'

export class ImageEditorSocket {
  private socket: BrowserSocket | null = null
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
    try {
      this.socket = new BrowserSocket('/image_editor', token, {
        disconnected: () => {
          this.socket = null
          this.scheduleRetry()
        },
        events: {
          IMAGE_EDITOR_READY: () => this.onReady(),
          IMAGE_EDITOR_UPDATE: data => this.onUpdate(data),
        },
      })
    } catch (error) {
      console.error(error)
      this.scheduleRetry()
    }
  }

  private scheduleRetry() {
    if (!this.stopped) this.retry = setTimeout(() => this.open(), 1000)
  }

  disconnect() {
    this.stopped = true
    this.native?.close()
    this.native = null
    clearTimeout(this.retry)
    const socket = this.socket
    this.socket = null
    socket?.close()
  }
}
