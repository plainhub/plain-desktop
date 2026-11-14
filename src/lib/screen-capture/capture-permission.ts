import type { CaptureInvoke } from './capture-client'

export async function requestScreenCapturePermission(
  invokeCommand?: CaptureInvoke,
): Promise<boolean> {
  const invoke = invokeCommand ?? (await import('@/lib/api/native-invoke')).invoke
  return Boolean(await invoke('screen_capture_request_permission'))
}

export async function openScreenCapturePermissionSettings(invokeCommand?: CaptureInvoke): Promise<void> {
  const invoke = invokeCommand ?? (await import('@/lib/api/native-invoke')).invoke
  await invoke('screen_capture_open_permission_settings')
}
