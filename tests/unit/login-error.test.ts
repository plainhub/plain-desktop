import { AuthErrorCode, parseAuthErrorCode } from '@/lib/api/auth-error-code'
import { expect, it } from 'vitest'
import { loginErrorMessage, loginErrorReason } from '../../src/lib/api/login-error'

it.each(['INVALID_PASSWORD', 'PEER_IDENTITY_CHANGED', 'INTERNAL_ERROR', 'TIMEOUT'])('preserves known %s from strings and Error objects', reason => {
  expect(loginErrorReason(reason)).toBe(reason)
  expect(loginErrorReason(new Error(reason))).toBe(reason)
  expect(loginErrorMessage(new Error(reason))).toBe(`login.${reason}`)
})

it('never exposes arbitrary close reasons or exception text', () => {
  expect(loginErrorReason(new Error('private server data'))).toBe('INVALID_AUTH_RESPONSE')
  expect(loginErrorMessage('unexpected remote content')).toBe('login.FAILED')
  expect(loginErrorMessage('FAILED_CONNECT_WS')).toBe('FAILED_CONNECT_WS')
  expect(loginErrorMessage('PROXY_AUTH_FAILED')).toBe('FAILED_CONNECT_WS')
})

it('accepts generated enum codes at both protocol boundaries', async () => {
  for (const reason of [AuthErrorCode.InvalidPassword, AuthErrorCode.PeerIdentityChanged, AuthErrorCode.RemoteAccessDisabled, AuthErrorCode.Rejected, AuthErrorCode.LoginExpired, AuthErrorCode.TooManyLoginAttempts]) {
    expect(parseAuthErrorCode(reason.toUpperCase())).toBe(reason)
    expect(loginErrorReason(reason)).toBe(reason)
  }
  expect(parseAuthErrorCode('unrecognized reason')).toBeUndefined()
  expect(parseAuthErrorCode('Invalid_Password')).toBeUndefined()
  expect(parseAuthErrorCode('invalid_password')).toBeUndefined()
  expect(parseAuthErrorCode('MOUNT_FAILED')).toBeUndefined()
  expect(parseAuthErrorCode('PASSWORD_REQUIRED')).toBeUndefined()
})

it('uses a structured GraphQL enum code independently of diagnostic prose', async () => {
  const { GqlError } = await import('../../src/lib/api/gql-error')
  const failure = GqlError.fromGraphql({ message: 'approval diagnostic', extensions: { code: 'REJECTED' } })
  expect(failure.message).toBe('approval diagnostic')
  expect(failure.code).toBe(AuthErrorCode.Rejected)
  expect(loginErrorReason(failure)).toBe(AuthErrorCode.Rejected)
  const unrelated = GqlError.fromGraphql({ message: 'INVALID_PASSWORD' })
  expect(loginErrorReason(unrelated)).toBe(AuthErrorCode.InternalError)
})

it('accepts only the public pairing error codes', async () => {
  const { PairingErrorCode, parsePairingErrorCode } = await import('../../src/lib/api/pairing-error-code')
  expect(Object.values(PairingErrorCode).sort()).toEqual([
    'INTERNAL_ERROR', 'INVALID_PASSWORD', 'PASSWORD_REQUIRED', 'PEER_IDENTITY_CHANGED',
    'REJECTED', 'TIMEOUT', 'TOO_MANY_LOGIN_ATTEMPTS',
  ])
  expect(parsePairingErrorCode('PASSWORD_REQUIRED')).toBe(PairingErrorCode.PasswordRequired)
  expect(parsePairingErrorCode('INVALID_PASSWORD')).toBe(PairingErrorCode.InvalidPassword)
  for (const value of ['invalid_password', 'MOUNT_FAILED', 'INVALID_PREF_VALUE', 'PAIRING_TRANSPORT_UNAVAILABLE']) expect(parsePairingErrorCode(value)).toBeUndefined()
})
