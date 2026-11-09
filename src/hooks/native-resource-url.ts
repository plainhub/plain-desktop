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

interface ResourceOwnerState {
  active: boolean
  entries: Set<ResourceEntry>
}
interface ResourceEntry {
  key: string
  path: string
  target: NativeTarget
  owners: Set<ResourceOwnerState>
  promise: Promise<string>
  resource?: INativeResource
  scope?: string
}
const handles = new Map<string, ResourceEntry>()

export interface NativeResourceOwner {
  url(path: string, size?: number, query?: string): Promise<string>
  retain(url: string): void
  dispose(): Promise<void>
}

export function nativeResourceReference(url: string): { path: string; target: NativeTarget } {
  const entry = [...handles.values()].find(entry => entry.resource?.url === url)
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

async function closeEntry(entry: ResourceEntry): Promise<void> {
  if (handles.get(entry.key) === entry) handles.delete(entry.key)
  if (!entry.resource || !entry.scope) return
  const resource = entry.resource
  entry.resource = undefined
  try {
    await invoke('native_resource_close', { id: resource.id, scope: entry.scope })
  } catch (error) {
    console.warn('native resource close failed', error)
  }
}

function forgetEntry(entry: ResourceEntry) {
  if (handles.get(entry.key) === entry) handles.delete(entry.key)
  for (const owner of entry.owners) owner.entries.delete(entry)
  entry.owners.clear()
}

export function createNativeResourceOwner(): NativeResourceOwner {
  const owner: ResourceOwnerState = { active: true, entries: new Set() }
  const checkOpen = () => { if (!owner.active) throw new Error('resource_closed') }
  const retain = (entry: ResourceEntry) => {
    checkOpen()
    owner.entries.add(entry)
    entry.owners.add(owner)
  }
  return {
    async url(path, size, query = '') {
      checkOpen()
      if (!path || !__IS_TAURI__ || /^(https?:|blob:|content:|ph:|fsid:)/i.test(path)) return ''
      const parameters = new URLSearchParams(query.replace(/^[?&]/, ''))
      const thumbnail = parameters.get('size') ?? undefined
      if (thumbnail !== undefined && thumbnail !== 'sm' && thumbnail !== 'lg') throw new Error('Invalid thumbnail size')
      if ([...parameters.keys()].some(key => key !== 'size' && key !== 'v')) throw new Error('Unsupported native resource parameter')
      if (!thumbnail && typeof size === 'number' && size > MAX_NATIVE_RESPONSE_BYTES) return ''
      const context = resourceContext()
      if (context.target.kind === 'peer' && !context.target.peerId) throw new Error('native_peer_id_missing')
      const contextKey = JSON.stringify(context)
      const key = JSON.stringify([context, path, thumbnail, parameters.get('v')])
      let entry = handles.get(key)
      if (!entry) {
        entry = { key, path, target: context.target, owners: new Set(), promise: Promise.resolve('') }
        handles.set(key, entry)
        retain(entry)
        const opened = entry
        opened.promise = (async () => {
          const checkCurrent = () => {
            if (!opened.owners.size || handles.get(key) !== opened) throw new Error('resource_closed')
            if (JSON.stringify(resourceContext()) !== contextKey) throw new Error('request_context_changed')
          }
          try {
            opened.scope = await nativeViewScope()
            checkCurrent()
            opened.resource = await invoke<INativeResource>('native_resource_open', { path, scope: opened.scope, thumbnail, target: context.target })
            checkCurrent()
            const resource = opened.resource
            const mime = resource.mime.split(';')[0].trim().toLowerCase()
            const image = ['image/jpeg', 'image/png', 'image/gif', 'image/webp', 'image/bmp', 'image/avif', 'image/x-icon'].includes(mime)
            const inline = resource.inlineAllowed && (context.target.kind === 'local' ? !mime.startsWith('image/') || image : image)
            // Native media schemes are not portable across WebViews; audio and video use Core HTTP.
            if (mime.startsWith('audio/') || mime.startsWith('video/') || !inline || resource.size > MAX_NATIVE_RESPONSE_BYTES) {
              await closeEntry(opened)
              forgetEntry(opened)
              return ''
            }
            return resource.url
          } catch (error) {
            await closeEntry(opened)
            forgetEntry(opened)
            throw error
          }
        })()
      } else retain(entry)
      const url = await entry.promise
      checkOpen()
      return url
    },
    retain(url) {
      const entry = [...handles.values()].find(entry => entry.resource?.url === url)
      if (!entry) throw new Error('Native resource is no longer available')
      retain(entry)
    },
    async dispose() {
      owner.active = false
      const closing: Promise<void>[] = []
      for (const entry of owner.entries) {
        entry.owners.delete(owner)
        if (!entry.owners.size) closing.push(closeEntry(entry))
      }
      owner.entries.clear()
      await Promise.all(closing)
    },
  }
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
