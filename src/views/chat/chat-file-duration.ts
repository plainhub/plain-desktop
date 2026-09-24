/** Chat message file duration in milliseconds; the wire stores seconds
 * (`durationSec`, legacy key `duration`) because old message rows cannot migrate.
 * Kept dependency-free so stores can import it without pulling view/tauri code. */
export function chatFileDurationMs(f: { durationSec?: number; duration?: number }): number {
  return (f.durationSec ?? f.duration ?? 0) * 1000
}
