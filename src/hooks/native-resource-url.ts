import { isLocalMode } from '@/lib/device/local-mode'
import { invoke } from '@tauri-apps/api/core'
import { nativeViewScope } from '@/lib/api/native-client'

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
const handles = new Map<string, { resource: INativeResource; scope: string }>()
const pending = new Map<string, Promise<string>>()

export function isNativeResourceUrl(url: string): boolean {
  try {
    const parsed = new URL(url)
    if (parsed.port || parsed.username || parsed.password) return false
    return (parsed.protocol === 'plain-resource:' && parsed.hostname === 'file')
      || ((parsed.protocol === 'http:' || parsed.protocol === 'https:') && parsed.hostname === 'plain-resource.file')
  } catch {
    return false
  }
}

/**
 * A reference the webview can load on its own. Feeds keep remote artwork as
 * an external link; everything else is a core reference.
 */
export function isExternalReference(reference: string): boolean {
  return !isNativeResourceUrl(reference) && /^https?:\/\//i.test(reference)
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
  if (!path || !isLocalMode()) {
    return ''
  }
  if (typeof size === 'number' && size > MAX_NATIVE_RESPONSE_BYTES) {
    return ''
  }
  const cached = handles.get(path)
  if (cached) {
    return cached.resource.url
  }
  const running = pending.get(path)
  if (running) {
    return running
  }
  const opened: Promise<string> = (async () => {
    const scope = await nativeViewScope()
    const checkCurrent = () => {
      if (pending.get(path) !== opened) throw new Error('resource_closed')
      if (!isLocalMode()) throw new Error('request_context_changed')
    }
    checkCurrent()
    const resource = await invoke<INativeResource>('native_resource_open', { path, scope })
    try {
      checkCurrent()
    } catch (error) {
      await invoke('native_resource_close', { id: resource.id, scope })
      throw error
    }
    handles.set(path, { resource, scope })
    return resource.url
  })().finally(() => {
    if (pending.get(path) === opened) pending.delete(path)
  })
  pending.set(path, opened)
  return opened
}

export async function closeNativeResource(path: string): Promise<void> {
  pending.delete(path)
  const cached = handles.get(path)
  if (!cached) {
    return
  }
  handles.delete(path)
  try {
    await invoke('native_resource_close', { id: cached.resource.id, scope: cached.scope })
  } catch (error) {
    console.warn('native resource close failed', error)
  }
}

export async function closeNativeResources(paths: Iterable<string>): Promise<void> {
  await Promise.all([...paths].map((path) => closeNativeResource(path)))
}
