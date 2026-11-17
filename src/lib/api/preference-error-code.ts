export enum PreferenceErrorCode {
  InvalidPrefValue = 'INVALID_PREF_VALUE',
  PreferenceUpdateFailed = 'PREFERENCE_UPDATE_FAILED',
}

const codes = new Set<string>(Object.values(PreferenceErrorCode))

export function parsePreferenceErrorCode(value: unknown): PreferenceErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as PreferenceErrorCode : undefined
}
