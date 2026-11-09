import { isLocalMode } from '@/lib/device/local-mode'
import { invoke } from '@tauri-apps/api/core'
import { nativeViewScope, type NativeTarget } from '@/lib/api/native-client'
import { getRemoteClientId } from '@/lib/device/client-id'
import { getLoginPeerRevision } from '@/lib/device/login-peers'

export interface INativeResource {
  id: string
  url: string
  name: string
  mime: string
  size: number
  inlineAllowed: boolean
}

// Handles stay open while a view may still show the file and are closed
// when that view drops them. The core evicts a handle when its window goes
// away, so a stale entry here can never keep a file open.
const handles = new Map<string, { path: string; target: NativeTarget; resource: INativeResource; scope: string }>()
const pending = new Map<string, { path: string; promise: Promise<string> }>()

export function nativeResourceReference(url: string): { path: string; target: NativeTarget } {
  const entry = [...handles.values()].find(entry => entry.resource.url === url)
  if (!entry) throw new Error('Native resource is no longer available')
  return { path: entry.path, target: entry.target }
}

function resourceContext() {
  const target: NativeTarget = isLocalMode() ? { kind: 'local' } : { kind: 'peer', peerId: getRemoteClientId() }
  return { target, revision: target.kind === 'peer' ? getLoginPeerRevision(target.peerId) : 0 }
}

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

// Only verified bounded resource types use the native scheme; other previews retain HTTP.
export async function nativeResourceUrl(path: string, size?: number, query = ''): Promise<string> {
  if (!path || !__IS_TAURI__ || /^(https?:|blob:|content:|ph:|fsid:)/i.test(path)) {
    return ''
  }
  const parameters = new URLSearchParams(query.replace(/^[?&]/, ''))
  const thumbnail = parameters.get('size') ?? undefined
  if (thumbnail !== undefined && thumbnail !== 'sm' && thumbnail !== 'lg') throw new Error('Invalid thumbnail size')
  if ([...parameters.keys()].some(key => key !== 'size' && key !== 'v')) throw new Error('Unsupported native resource parameter')
  if (!thumbnail && typeof size === 'number' && size > MAX_NATIVE_RESPONSE_BYTES) return ''
  const context = resourceContext()
  if (context.target.kind === 'peer' && !context.target.peerId) throw new Error('native_peer_id_missing')
  const contextKey = JSON.stringify(context)
  const key = JSON.stringify([context, path, thumbnail, parameters.get('v')])
  const cached = handles.get(key)
  if (cached) {
    return cached.resource.url
  }
  const running = pending.get(key)
  if (running) {
    return running.promise
  }
  const opened: Promise<string> = (async () => {
    const scope = await nativeViewScope()
    const checkCurrent = () => {
      if (pending.get(key)?.promise !== opened) throw new Error('resource_closed')
      if (JSON.stringify(resourceContext()) !== contextKey) throw new Error('request_context_changed')
    }
    checkCurrent()
    const resource = await invoke<INativeResource>('native_resource_open', { path, scope, thumbnail, target: context.target })
    try {
      checkCurrent()
    } catch (error) {
      await invoke('native_resource_close', { id: resource.id, scope })
      throw error
    }
    const mime = resource.mime.split(';')[0].trim().toLowerCase()
    const image = ['image/jpeg', 'image/png', 'image/gif', 'image/webp', 'image/bmp', 'image/avif', 'image/x-icon'].includes(mime)
    const inline = resource.inlineAllowed && (context.target.kind === 'local' ? !mime.startsWith('image/') || image : image)
    if (!inline || resource.size > MAX_NATIVE_RESPONSE_BYTES) {
      await invoke('native_resource_close', { id: resource.id, scope })
      return ''
    }
    handles.set(key, { path, target: context.target, resource, scope })
    return resource.url
  })().finally(() => {
    if (pending.get(key)?.promise === opened) pending.delete(key)
  })
  pending.set(key, { path, promise: opened })
  return opened
}

export async function closeNativeResource(path: string): Promise<void> {
  for (const [key, entry] of pending) {
    if (entry.path === path) pending.delete(key)
  }
  const closing = [...handles].filter(([, entry]) => entry.path === path)
  await Promise.all(closing.map(async ([key, cached]) => {
    handles.delete(key)
    try {
      await invoke('native_resource_close', { id: cached.resource.id, scope: cached.scope })
    } catch (error) {
      console.warn('native resource close failed', error)
    }
  }))
}

export async function closeNativeResources(paths: Iterable<string>): Promise<void> {
  await Promise.all([...paths].map((path) => closeNativeResource(path)))
}

export async function nativeResourceImage(path: string, signal: AbortSignal): Promise<Blob> {
  const context = resourceContext()
  const contextKey = JSON.stringify(context)
  const checkCurrent = () => {
    signal.throwIfAborted()
    if (JSON.stringify(resourceContext()) !== contextKey) throw new Error('request_context_changed')
  }
  const scope = await nativeViewScope()
  checkCurrent()
  const resource = await invoke<INativeResource>('native_resource_open', { path, scope, target: context.target })
  let closing: Promise<unknown> | undefined
  const close = () => closing ??= invoke('native_resource_close', { id: resource.id, scope })
  const abort = () => { void close().catch(error => console.warn('native image close failed', error)) }
  signal.addEventListener('abort', abort, { once: true })
  try {
    checkCurrent()
    const mime = resource.mime.split(';')[0].trim().toLowerCase()
    if (!mime.startsWith('image/')) throw new Error('Image resource required')
    if (resource.size > MAX_NATIVE_RESPONSE_BYTES) throw new Error('Image exceeds native editing limit')
    const parts: ArrayBuffer[] = []
    for (let offset = 0; offset < resource.size;) {
      checkCurrent()
      const length = Math.min(64 * 1024, resource.size - offset)
      const bytes = await invoke<ArrayBuffer>('native_resource_read', { id: resource.id, scope, offset, length })
      checkCurrent()
      if (!(bytes instanceof ArrayBuffer) || bytes.byteLength !== length) throw new Error('Image resource read length mismatch')
      parts.push(bytes)
      offset += bytes.byteLength
    }
    return new Blob(parts, { type: mime })
  } finally {
    signal.removeEventListener('abort', abort)
    await close()
  }
}
