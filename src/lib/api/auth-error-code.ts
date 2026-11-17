export enum AuthErrorCode {
  Canceled = 'CANCELED',
  InternalError = 'INTERNAL_ERROR',
  InvalidLoginRequest = 'INVALID_LOGIN_REQUEST',
  InvalidPassword = 'INVALID_PASSWORD',
  PasswordSetupRequired = 'PASSWORD_SETUP_REQUIRED',
  PeerIdentityChanged = 'PEER_IDENTITY_CHANGED',
  Rejected = 'REJECTED',
  RemoteAccessDisabled = 'REMOTE_ACCESS_DISABLED',
  TooManyLoginAttempts = 'TOO_MANY_LOGIN_ATTEMPTS',
  Timeout = 'TIMEOUT',
}

const codes = new Set<string>(Object.values(AuthErrorCode))

export function parseAuthErrorCode(value: unknown): AuthErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as AuthErrorCode : undefined
}
