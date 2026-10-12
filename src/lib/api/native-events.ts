import { Channel, invoke } from '@tauri-apps/api/core'

export interface NativeEventStream {
  /** Stop delivering frames. The backend channel ends when its command
   *  returns, so this does not need a transport-level abort. */
  close(): void
}

/** Subscribes to this machine's shared Rust core event feed. Frames are the
 *  same bytes the local chat socket delivered: `EVENT_NAME` + NUL + payload,
 *  with binary payloads left raw and text payloads token-encrypted. */
export function subscribeNativeEvents(onFrame: (frame: ArrayBuffer) => void, onClosed: () => void): NativeEventStream {
  let closed = false
  const channel = new Channel<ArrayBuffer>()
  channel.onmessage = (frame) => {
    if (!closed) onFrame(frame)
  }

  void invoke('native_events', { onEvent: channel })
    .catch((error) => {
      if (!closed) console.warn('native events stream failed', error)
    })
    .finally(() => {
      if (closed) return
      closed = true
      onClosed()
    })

  return {
    close() {
      closed = true
    },
  }
}
