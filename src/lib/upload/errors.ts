// Upload error strings come from three worlds: GqlError i18n keys
// (connection_timeout…), literal client strings set in upload.ts
// ('Network error', 'Failed to merge chunks'…), and raw server response
// bodies. Map the known ones to i18n keys for display; anything else
// (server text) is shown as-is.

const CHUNK_FAILED_RE = /^Failed to upload: chunk/
const MERGE_SIZE_RE = /^Server merged size/

const ERROR_KEYS: Record<string, string> = {
  connection_timeout: 'connection_timeout',
  network_error: 'network_error',
  'Network error': 'network_error',
  desktop_access_disabled: 'desktop_access_disabled',
  unauthorized: 'upload_unauthorized',
  'Failed to merge chunks': 'upload_merge_failed',
}

export function uploadErrorKey(raw: string): string | undefined {
  if (!raw) return undefined
  const direct = ERROR_KEYS[raw]
  if (direct) return direct
  if (CHUNK_FAILED_RE.test(raw)) return 'upload_chunk_failed'
  if (MERGE_SIZE_RE.test(raw)) return 'upload_merge_size_mismatch'
  return undefined
}

// Errors worth retrying automatically: they indicate a transport-level
// hiccup, not a verdict about the file itself. Server-rejected uploads
// (unknown strings) and auth failures would fail the same way again.
export function isTransientUploadError(raw: string): boolean {
  if (!raw) return false
  const key = uploadErrorKey(raw)
  return key === 'connection_timeout' || key === 'network_error' || key === 'upload_chunk_failed' || key === 'upload_merge_failed'
}

export function uploadErrorText(t: (key: string) => string, raw: string): string {
  const key = uploadErrorKey(raw)
  return key ? t(key) : raw
}
