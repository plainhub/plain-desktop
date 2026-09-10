import type { CaptureInvoke } from './capture-client'

export async function requestScreenCapturePermission(
  invokeCommand?: CaptureInvoke,
): Promise<boolean> {
  const invoke = invokeCommand ?? (await import('@tauri-apps/api/core')).invoke
  return Boolean(await invoke('screen_capture_request_permission'))
}

export async function openScreenCapturePermissionSettings(invokeCommand?: CaptureInvoke): Promise<void> {
  const invoke = invokeCommand ?? (await import('@tauri-apps/api/core')).invoke
  await invoke('screen_capture_open_permission_settings')
}
