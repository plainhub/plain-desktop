// IPC from Tauri plugins-workspace a2364a5f216324439feedeb25b2db74e7b1eba90 (tauri-plugins.LICENSE).
import { invoke } from '@tauri-apps/api/core'

export function open(options: { multiple: true; filters: { name: string; extensions: string[] }[] }): Promise<string | string[] | null> {
  return invoke('plugin:dialog|open', { options: Object.freeze(options) })
}

export function save(options: { defaultPath: string }): Promise<string | null> {
  return invoke('plugin:dialog|save', { options: Object.freeze(options) })
}

export function openUrl(url: string): Promise<void> {
  return invoke('plugin:opener|open_url', { url })
}

export function revealItemInDir(path: string): Promise<void> {
  return invoke('plugin:opener|reveal_item_in_dir', { paths: [path] })
}

export async function isPermissionGranted(): Promise<boolean> {
  const permission = window.Notification.permission
  return permission === 'default' ? invoke('plugin:notification|is_permission_granted') : permission === 'granted'
}

export function requestPermission(): Promise<NotificationPermission> {
  return window.Notification.requestPermission()
}
