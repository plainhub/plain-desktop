import { expect, it } from 'vitest'
import { loginErrorMessage, loginErrorReason } from '../../src/lib/api/login-error'

it.each(['invalid_password', 'peer_identity_changed', 'internal_error', 'timeout'])('preserves known %s from strings and Error objects', reason => {
  expect(loginErrorReason(reason)).toBe(reason)
  expect(loginErrorReason(new Error(reason))).toBe(reason)
  expect(loginErrorMessage(new Error(reason))).toBe(`login.${reason}`)
})

it('never exposes arbitrary close reasons or exception text', () => {
  expect(loginErrorReason(new Error('private server data'))).toBe('invalid_auth_response')
  expect(loginErrorMessage('unexpected remote content')).toBe('login.failed')
  expect(loginErrorMessage('failed_connect_ws')).toBe('failed_connect_ws')
  expect(loginErrorMessage('proxy_auth_failed')).toBe('failed_connect_ws')
})

it('accepts generated enum codes at both protocol boundaries', async () => {
  const { ErrorCode, parseErrorCode } = await import('../../src/lib/api/error-code')
  for (const reason of [ErrorCode.InvalidPassword, ErrorCode.PeerIdentityChanged, ErrorCode.RemoteAccessDisabled, ErrorCode.Rejected, ErrorCode.LoginExpired, ErrorCode.TooManyLoginAttempts]) {
    expect(parseErrorCode(reason.toUpperCase())).toBe(reason)
    expect(loginErrorReason(reason)).toBe(reason)
  }
  expect(parseErrorCode('unrecognized reason')).toBeUndefined()
  expect(parseErrorCode('Invalid_Password')).toBeUndefined()
})

it('uses a structured GraphQL enum code independently of diagnostic prose', async () => {
  const { ErrorCode } = await import('../../src/lib/api/error-code')
  const { GqlError } = await import('../../src/lib/api/gql-error')
  const failure = GqlError.fromGraphql({ message: 'approval diagnostic', extensions: { code: 'rejected' } })
  expect(failure.message).toBe('approval diagnostic')
  expect(failure.code).toBe(ErrorCode.Rejected)
  expect(loginErrorReason(failure)).toBe(ErrorCode.Rejected)
  const unrelated = GqlError.fromGraphql({ message: 'invalid_password' })
  expect(loginErrorReason(unrelated)).toBe(ErrorCode.InternalError)
})
