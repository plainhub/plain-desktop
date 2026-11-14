import { invoke as tauriInvoke } from '@tauri-apps/api/core'
export * from '@tauri-apps/api/core'

const pending = new Set<Promise<unknown>>()
const closing = new Set<() => void>()
let reloading = false

export function invoke<T>(...args: Parameters<typeof tauriInvoke>): Promise<T> {
  // New work belongs to the next document once navigation has started.
  if (reloading) return new Promise(() => {})
  const request = tauriInvoke<T>(...args)
  pending.add(request)
  const settled = () => { pending.delete(request) }
  void request.then(settled, settled)
  return request
}

export function onNativeReload(close: () => void): () => void {
  closing.add(close)
  return () => { closing.delete(close) }
}

export function installNativeReload(): void {
  window.addEventListener('native-reload', () => {
    if (reloading) return
    for (const close of closing) close()
    reloading = true
    void Promise.allSettled([...pending])
      .then(() => tauriInvoke('native_reload_ready'))
      .then(() => window.location.reload())
      .catch(error => console.error('Native reload failed', error))
  })
}
