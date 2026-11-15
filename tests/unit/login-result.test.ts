import { expect, it } from 'vitest'
import { completeLogin } from '../../src/lib/api/login-result'

it('returns the server deviceId without confusing it with the caller clientId', () => {
  expect(completeLogin({ status: 'COMPLETED', deviceId: '706c6169-3dee-8e04-918a-58062a09d734' }, 'signature', 'session-key', '706c6169-3dee-8e04-918a-58062a09d734')).toEqual({
    deviceId: '706c6169-3dee-8e04-918a-58062a09d734', token: 'session-key', signaturePublicKey: 'signature', chatPaired: false,
  })
})

it('rejects the removed clientId response field', () => {
  expect(() => completeLogin({ status: 'COMPLETED', clientId: '706c6169-3dee-8e04-918a-58062a09d734' } as never, 'signature', 'session-key')).toThrow('invalid_auth_response')
})

it('rejects a different server identity', () => {
  expect(() => completeLogin({ status: 'COMPLETED', deviceId: 'other-device' }, 'signature', 'session-key', '706c6169-3dee-8e04-918a-58062a09d734')).toThrow('peer_identity_changed')
})
