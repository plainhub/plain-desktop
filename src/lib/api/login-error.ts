const loginErrors = new Set([
  'canceled', 'timeout', 'failed_connect_ws', 'proxy_auth_failed',
  'secure_login_unavailable', 'invalid_auth_response', 'invalid_login_request',
  'invalid_password', 'peer_identity_changed', 'signature_verification_failed',
  'remote_access_disabled', 'login_expired', 'rejected', 'too_many_login_attempts',
  'internal_error', 'unauthorized', 'chat_pairing_unavailable',
])

export function loginErrorReason(error: unknown, fallback = 'invalid_auth_response'): string {
  const reason = error instanceof Error ? error.message : error
  return typeof reason === 'string' && loginErrors.has(reason) ? reason : fallback
}

export function loginErrorMessage(error: unknown): string {
  const reason = loginErrorReason(error, 'failed')
  if (reason === 'proxy_auth_failed') return 'failed_connect_ws'
  return ['remote_access_disabled', 'failed_connect_ws'].includes(reason) ? reason : `login.${reason}`
}
