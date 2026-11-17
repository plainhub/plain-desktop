export enum ClipboardErrorCode {
  InvalidClipboardText = 'INVALID_CLIPBOARD_TEXT',
  ClipboardSyncDisabled = 'CLIPBOARD_SYNC_DISABLED',
}

const codes = new Set<string>(Object.values(ClipboardErrorCode))

export function parseClipboardErrorCode(value: unknown): ClipboardErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as ClipboardErrorCode : undefined
}
