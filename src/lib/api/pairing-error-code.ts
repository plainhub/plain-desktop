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

export function pairingErrorMessage(error: unknown): string {
  const value = error && typeof error === 'object' && 'code' in error ? error.code : error instanceof Error ? error.message : error
  switch (parsePairingErrorCode(value)) {
    case PairingErrorCode.Timeout: return 'pair_expired'
    case PairingErrorCode.Rejected: return 'pair_rejected'
    case PairingErrorCode.TooManyLoginAttempts: return 'pair_too_many_attempts'
    case PairingErrorCode.InvalidPassword: return 'login.INVALID_PASSWORD'
    default: return 'pair_failed'
  }
}
