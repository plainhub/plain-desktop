import type { CaptureInvoke } from './capture-client'

export async function openScreenCapturePermissionSettings(invokeCommand?: CaptureInvoke): Promise<void> {
  const invoke = invokeCommand ?? (await import('@tauri-apps/api/core')).invoke
  await invoke('screen_capture_open_permission_settings')
}
