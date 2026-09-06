import type { Event } from '@tauri-apps/api/event'

export type CaptureWindowEventHandler<T> = (event: Event<T>) => void | Promise<void>

export type CaptureWindowListen = <T>(
  event: string,
  handler: CaptureWindowEventHandler<T>,
  options?: { target?: string }
) => Promise<() => void>

/**
 * Tauri's default listener target is `Any`, which does not receive events
 * emitted with `emit_to(label, ...)`. Capture traffic is deliberately
 * label-targeted, so every listener must opt into that exact label too.
 */
export function listenToCaptureWindow<T>(
  listen: CaptureWindowListen,
  windowLabel: string,
  event: string,
  handler: CaptureWindowEventHandler<T>
): Promise<() => void> {
  if (!windowLabel.trim()) return Promise.reject(new Error('capture window label is required'))
  return listen(event, handler, { target: windowLabel })
}
