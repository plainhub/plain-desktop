export enum PairingErrorCode {
  Canceled = 'CANCELED',
  Timeout = 'TIMEOUT',
  InternalError = 'INTERNAL_ERROR',
  Rejected = 'REJECTED',
  BleConnectionClosing = 'BLE_CONNECTION_CLOSING',
  BlePairingCapacityExceeded = 'BLE_PAIRING_CAPACITY_EXCEEDED',
  FailedConnect = 'FAILED_CONNECT',
  PairingTransportUnavailable = 'PAIRING_TRANSPORT_UNAVAILABLE',
  PairingAddressUnavailable = 'PAIRING_ADDRESS_UNAVAILABLE',
  FailedConnectWs = 'FAILED_CONNECT_WS',
  InvalidAuthResponse = 'INVALID_AUTH_RESPONSE',
  InvalidSession = 'INVALID_SESSION',
  PasswordRequired = 'PASSWORD_REQUIRED',
  PeerIdentityChanged = 'PEER_IDENTITY_CHANGED',
  RemoteAccessDisabled = 'REMOTE_ACCESS_DISABLED',
  SignatureVerificationFailed = 'SIGNATURE_VERIFICATION_FAILED',
  ChatPairingUnavailable = 'CHAT_PAIRING_UNAVAILABLE',
  InvalidPassword = 'INVALID_PASSWORD',
  LoginExpired = 'LOGIN_EXPIRED',
  TooManyLoginAttempts = 'TOO_MANY_LOGIN_ATTEMPTS',
  PasswordSetupRequired = 'PASSWORD_SETUP_REQUIRED',
  SecureLoginUnavailable = 'SECURE_LOGIN_UNAVAILABLE',
  InvalidLoginRequest = 'INVALID_LOGIN_REQUEST',
  TooManyLoginConnections = 'TOO_MANY_LOGIN_CONNECTIONS',
}

const codes = new Set<string>(Object.values(PairingErrorCode))

export function parsePairingErrorCode(value: unknown): PairingErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as PairingErrorCode : undefined
}
