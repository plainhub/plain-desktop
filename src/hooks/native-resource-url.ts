import { invoke } from '@tauri-apps/api/core'
import { isLocalModeAllowed } from '@/lib/device/local-mode'

export interface INativeResource {
  id: string
  url: string
  name: string
  mime: string
  size: number
}

// Handles stay open while a view may still show the file and are closed
// when that view drops them. The core evicts a handle when its window goes
// away, so a stale entry here can never keep a file open.
const handles = new Map<string, string>()
const pending = new Map<string, Promise<string>>()

export function isNativeResourceUrl(url: string): boolean {
  return url.startsWith('plain-resource://')
}

/**
 * A reference the webview can load on its own. Feeds keep remote artwork as
 * an external link; everything else is a core reference.
 */
export function isExternalReference(reference: string): boolean {
  return /^https?:\/\//i.test(reference)
}

/**
 * Tauri 2 has no streaming protocol response, so a single read is buffered
 * in full. Files larger than this window keep their existing transport
 * rather than being refused mid-playback; the Rust side applies the same
 * bound in `MAX_RESPONSE_BYTES`.
 */
export const MAX_NATIVE_RESPONSE_BYTES = 32 * 1024 * 1024

/**
 * Resolve a file reference to this machine's native resource URL.
 *
 * The reference is whatever the shared core understands — a plaintext
 * absolute path, a `fid:` file id or an `app://` path — and the core both
 * resolves and authorizes it, so an unsupported or unauthorized reference
 * fails here instead of quietly falling back to another transport.
 *
 * Returns '' only when this app is not talking to its own core (remote
 * target or web build) or when the file is too large for the protocol's
 * response window; both are transport decisions, not reference failures.
 */
export async function nativeResourceUrl(path: string, size?: number): Promise<string> {
  if (!path || !isLocalModeAllowed()) {
    return ''
  }
  if (typeof size === 'number' && size > MAX_NATIVE_RESPONSE_BYTES) {
    return ''
  }
  const cached = handles.get(path)
  if (cached) {
    return cached
  }
  const running = pending.get(path)
  if (running) {
    return running
  }
  const opened = invoke<INativeResource>('native_resource_open', { path }).then((resource) => {
    pending.delete(path)
    handles.set(path, resource.url)
    return resource.url
  })
  pending.set(path, opened)
  return opened
}

export async function closeNativeResource(path: string): Promise<void> {
  const cached = handles.get(path)
  if (!cached) {
    return
  }
  handles.delete(path)
  const id = cached.slice(cached.lastIndexOf('/') + 1)
  if (!id) {
    return
  }
  try {
    await invoke('native_resource_close', { id })
  } catch (error) {
    console.warn('native resource close failed', error)
  }
}

export async function closeNativeResources(paths: Iterable<string>): Promise<void> {
  await Promise.all([...paths].map((path) => closeNativeResource(path)))
}
