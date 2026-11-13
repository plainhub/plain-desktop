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
