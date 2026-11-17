export enum NativeErrorCode {
  NativeError = 'NATIVE_ERROR',
  NativePeerIdMissing = 'NATIVE_PEER_ID_MISSING',
}

const codes = new Set<string>(Object.values(NativeErrorCode))

export function parseNativeErrorCode(value: unknown): NativeErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as NativeErrorCode : undefined
}
