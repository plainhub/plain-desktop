export enum AccessErrorCode {
  RemoteAccessDisabled = 'REMOTE_ACCESS_DISABLED',
  NoPermission = 'NO_PERMISSION',
  Unauthorized = 'UNAUTHORIZED',
  Forbidden = 'FORBIDDEN',
  UnsupportedCapability = 'UNSUPPORTED_CAPABILITY',
}

const codes = new Set<string>(Object.values(AccessErrorCode))

export function parseAccessErrorCode(value: unknown): AccessErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as AccessErrorCode : undefined
}
