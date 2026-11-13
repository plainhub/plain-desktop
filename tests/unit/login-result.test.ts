import { expect, it } from 'vitest'
import { completeLogin } from '../../src/lib/api/login-result'

it('returns the server deviceId without confusing it with the caller clientId', () => {
  expect(completeLogin({ status: 'COMPLETED', deviceId: 'server-device' }, 'signature', 'session-key', 'server-device')).toEqual({
    deviceId: 'server-device', token: 'session-key', signaturePublicKey: 'signature', chatPaired: false,
  })
})

it('rejects the removed clientId response field', () => {
  expect(() => completeLogin({ status: 'COMPLETED', clientId: 'server-device' } as never, 'signature', 'session-key')).toThrow('invalid_auth_response')
})

it('rejects a different server identity', () => {
  expect(() => completeLogin({ status: 'COMPLETED', deviceId: 'other-device' }, 'signature', 'session-key', 'server-device')).toThrow('peer_identity_changed')
})
