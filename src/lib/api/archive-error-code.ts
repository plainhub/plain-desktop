export enum ArchiveErrorCode {
  ArchiveEntryUnavailable = 'ARCHIVE_ENTRY_UNAVAILABLE',
}

const codes = new Set<string>(Object.values(ArchiveErrorCode))

export function parseArchiveErrorCode(value: unknown): ArchiveErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as ArchiveErrorCode : undefined
}
