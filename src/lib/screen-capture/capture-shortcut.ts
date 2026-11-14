import type { CaptureInvoke } from './capture-client'

export interface CaptureShortcutStatus {
  registered: boolean
  accelerator: string
  error: string | null
}

export async function getCaptureShortcutStatus(
  invokeCommand?: CaptureInvoke,
): Promise<CaptureShortcutStatus> {
  const invoke = invokeCommand ?? (await import('@/lib/api/native-invoke')).invoke
  return (await invoke('screen_capture_shortcut_status')) as CaptureShortcutStatus
}

export async function setCaptureShortcut(
  accelerator: string | null,
  invokeCommand?: CaptureInvoke,
): Promise<CaptureShortcutStatus> {
  const invoke = invokeCommand ?? (await import('@/lib/api/native-invoke')).invoke
  return (await invoke('screen_capture_set_shortcut', { accelerator })) as CaptureShortcutStatus
}
