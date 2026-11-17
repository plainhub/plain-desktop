export enum ShareAccessErrorCode {
  PasswordRequired = 'PASSWORD_REQUIRED',
}

const codes = new Set<string>(Object.values(ShareAccessErrorCode))

export function parseShareAccessErrorCode(value: unknown): ShareAccessErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as ShareAccessErrorCode : undefined
}
