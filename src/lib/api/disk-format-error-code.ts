export enum DiskFormatErrorCode {
  FormatDiskFailed = 'FORMAT_DISK_FAILED',
}

const codes = new Set<string>(Object.values(DiskFormatErrorCode))

export function parseDiskFormatErrorCode(value: unknown): DiskFormatErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as DiskFormatErrorCode : undefined
}
