/** Chat message file duration in milliseconds. The wire key is `durationMs`;
 * legacy rows stored seconds under `duration` and are converted here — the
 * only place in the app that multiplies. Kept dependency-free so stores can
 * import it without pulling view/tauri code. */
export function chatFileDurationMs(f: { durationMs?: number; duration?: number }): number {
  return f.durationMs ?? (f.duration ?? 0) * 1000
}
