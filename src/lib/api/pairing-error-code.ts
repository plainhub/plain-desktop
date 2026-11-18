export enum PairingErrorCode {
  Timeout = 'TIMEOUT',
  InternalError = 'INTERNAL_ERROR',
  Rejected = 'REJECTED',
  PasswordRequired = 'PASSWORD_REQUIRED',
  PeerIdentityChanged = 'PEER_IDENTITY_CHANGED',
  InvalidPassword = 'INVALID_PASSWORD',
  TooManyLoginAttempts = 'TOO_MANY_LOGIN_ATTEMPTS',
}

const codes = new Set<string>(Object.values(PairingErrorCode))

export function parsePairingErrorCode(value: unknown): PairingErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as PairingErrorCode : undefined
}
