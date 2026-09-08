import { invoke } from '@tauri-apps/api/core'
import type { ISource } from '@/components/lightbox/types'
import { getRemoteClientId } from '@/lib/device/client-id'

/**
 * Open a new app window at the given router path (e.g. "/messages").
 * If a window at that exact path is already open, it receives focus instead.
 * No-op outside Tauri.
 *
 * Appends `?__cid=<bound-device-id>` so the child window inherits the
 * parent's binding on bootstrap via `applyUrlClientId()`. Only forwards
 * the id when the parent is actually bound to a device — in local mode
 * the child must also start unbound, otherwise `getCurrentAuthToken()`
 * in the child would look up a device that doesn't exist in the sessions
 * list (desktop clientId ≠ any Android device) and the router guard
 * would redirect the child to /login.
 */
export async function openWindow(path: string): Promise<void> {
  if (!__IS_TAURI__) return
  const cid = getRemoteClientId()
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
  await invoke('media_preview_activate', { source })
}
