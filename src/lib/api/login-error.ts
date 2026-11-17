import { ErrorCode, parseErrorCode } from './error-code'

const loginErrors = new Set<ErrorCode>([
  ErrorCode.Canceled, ErrorCode.Timeout, ErrorCode.FailedConnectWs, ErrorCode.ProxyAuthFailed,
  ErrorCode.SecureLoginUnavailable, ErrorCode.InvalidAuthResponse, ErrorCode.InvalidLoginRequest,
  ErrorCode.InvalidPassword, ErrorCode.PeerIdentityChanged, ErrorCode.SignatureVerificationFailed,
  ErrorCode.RemoteAccessDisabled, ErrorCode.LoginExpired, ErrorCode.Rejected, ErrorCode.TooManyLoginAttempts,
  ErrorCode.InternalError, ErrorCode.Unauthorized, ErrorCode.ChatPairingUnavailable,
])

export function loginErrorReason(error: unknown, fallback = ErrorCode.InvalidAuthResponse): ErrorCode {
  const code = parseErrorCode(error && typeof error === 'object' && 'code' in error ? error.code : error instanceof Error ? error.message : error)
  return code && loginErrors.has(code) ? code : fallback
}

export function loginErrorMessage(error: unknown): string {
  const reason = loginErrorReason(error, ErrorCode.Failed)
  if (reason === ErrorCode.ProxyAuthFailed) return ErrorCode.FailedConnectWs
  return reason === ErrorCode.RemoteAccessDisabled || reason === ErrorCode.FailedConnectWs ? reason : `login.${reason}`
}
