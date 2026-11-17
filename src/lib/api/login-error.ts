import { AuthErrorCode, parseAuthErrorCode } from '@/lib/api/auth-error-code'

export function loginErrorReason(error: unknown, fallback = AuthErrorCode.InvalidAuthResponse): AuthErrorCode {
  const code = parseAuthErrorCode(error && typeof error === 'object' && 'code' in error ? error.code : error instanceof Error ? error.message : error)
  return code ?? fallback
}

export function loginErrorMessage(error: unknown): string {
  const reason = loginErrorReason(error, AuthErrorCode.Failed)
  if (reason === AuthErrorCode.ProxyAuthFailed) return AuthErrorCode.FailedConnectWs
  return reason === AuthErrorCode.RemoteAccessDisabled || reason === AuthErrorCode.FailedConnectWs ? reason : `login.${reason}`
}
