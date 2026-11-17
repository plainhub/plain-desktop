export enum MountErrorCode {
  MountFailed = 'MOUNT_FAILED',
}

const codes = new Set<string>(Object.values(MountErrorCode))

export function parseMountErrorCode(value: unknown): MountErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as MountErrorCode : undefined
}
