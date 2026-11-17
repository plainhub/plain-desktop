export enum KeyValueErrorCode {
  InvalidKeyValue = 'INVALID_KEY_VALUE',
}

const codes = new Set<string>(Object.values(KeyValueErrorCode))

export function parseKeyValueErrorCode(value: unknown): KeyValueErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as KeyValueErrorCode : undefined
}
