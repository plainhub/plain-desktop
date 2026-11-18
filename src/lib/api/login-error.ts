import { AuthErrorCode, parseAuthErrorCode } from '@/lib/api/auth-error-code'

export function loginErrorReason(error: unknown): AuthErrorCode {
  const code = parseAuthErrorCode(error && typeof error === 'object' && 'code' in error ? error.code : error instanceof Error ? error.message : error)
  return code ?? AuthErrorCode.InternalError
}

export function loginErrorMessage(error: unknown): string {
  if (error instanceof DOMException && error.name === 'NotSupportedError') return 'login.browser_unsupported'
  const reason = loginErrorReason(error)
  if (reason === AuthErrorCode.RemoteAccessDisabled) return reason
  if (reason === AuthErrorCode.PasswordSetupRequired || reason === AuthErrorCode.Canceled) return `event_reason.${reason}`
  return `login.${reason}`
}
