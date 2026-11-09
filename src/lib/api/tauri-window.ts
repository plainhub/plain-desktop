import { invoke } from '@tauri-apps/api/core'
import type { ISource } from '@/components/lightbox/types'
import { getRemoteClientId } from '@/lib/device/client-id'
import { isNativeResourceUrl, nativeResourceReference } from '@/hooks/native-resource-url'

/**
 * Open a new app window at the given router path (e.g. "/messages").
 * If a window at that exact path is already open, it receives focus instead.
 * No-op outside Tauri.
 *
 * Pass a device ID to open on that device, or an empty ID to open locally.
 */
export async function openWindow(path: string, cid = getRemoteClientId()): Promise<void> {
  if (!__IS_TAURI__) return
  const sep = path.includes('?') ? '&' : '?'
  const finalPath =
    cid
      ? `${path}${sep}__cid=${encodeURIComponent(cid)}`
      : path
  await invoke('open_window', { path: finalPath })
}

/**
 * Open the About window. If it is already open, it receives focus instead.
 * No-op outside Tauri.
 */
export async function openAboutWindow(): Promise<void> {
  if (!__IS_TAURI__) return
  await invoke('open_about_window')
}

/**
 * Update the display name shown for this window in the macOS dock right-click menu.
 * Should be called whenever the active device session changes.
 * No-op outside Tauri or on non-macOS platforms.
 */
export async function setWindowDeviceName(name: string): Promise<void> {
  if (!__IS_TAURI__) return
  await invoke('set_window_device_name', { name })
}

export async function openMediaInWindow(source: ISource): Promise<void> {
  if (!__IS_TAURI__) return
  let peerId = getRemoteClientId()
  if (isNativeResourceUrl(source.src)) {
    const reference = nativeResourceReference(source.src)
    peerId = reference.target.kind === 'peer' ? reference.target.peerId : ''
    source = { ...source, path: reference.path, src: '', playbackUrl: undefined, transcoded: undefined }
  }
  if (source.thumbnail && isNativeResourceUrl(source.thumbnail)) source = { ...source, thumbnail: undefined }
  await invoke('media_preview_activate', { source: { ...source, __cid: peerId } })
}

export async function openImageEditor(source: Pick<ISource, 'src' | 'path' | 'name'>): Promise<void> {
  const query = new URLSearchParams({ name: source.name || '' })
  if (isNativeResourceUrl(source.src)) {
    const reference = nativeResourceReference(source.src)
    query.set('reference', reference.path)
    await openWindow(`/image-editor/new?${query}`, reference.target.kind === 'peer' ? reference.target.peerId : '')
  } else {
    query.set('src', source.src)
    const peerId = getRemoteClientId()
    if (__IS_TAURI__ && peerId) query.set('__cid', peerId)
    window.open(`/image-editor/new?${query}`, '_blank')
  }
}
