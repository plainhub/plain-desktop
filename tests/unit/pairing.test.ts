import { beforeEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ native: vi.fn(), listeners: new Map<string, (value: any) => void>() }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: mock.native }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe: (name: string, fn: (value: unknown) => void) => { mock.listeners.set(name, fn); return () => mock.listeners.delete(name) },
} }))
import { createPairing, pairingViews } from '../../src/lib/device/pairing'

beforeEach(() => {
  vi.resetAllMocks()
  mock.listeners.clear()
  pairingViews.clear()
  mock.native.mockResolvedValue({ data: { pairPeer: true, cancelPairing: true, respondToPairing: true } })
})
it('sends the peer and required mode and uses notifications instead of state queries', async () => {
  const pairing = createPairing()
  await pairing.start({ peerId: 'peer', mode: 'MANAGE' })
  expect(mock.native.mock.lastCall?.[0].variables).toEqual({ input: { peerId: 'peer', mode: 'MANAGE' } })
  mock.listeners.get('pairingStarted')!({ deviceId: 'peer', verificationCode: '01234567' })
  expect(pairing.state.value?.verificationCode).toBe('01234567')
  mock.listeners.get('pairingCompleted')!({ deviceId: 'peer' })
  expect(pairing.state.value?.phase).toBe('SUCCEEDED')
  expect(mock.native).toHaveBeenCalledTimes(1)
  pairing.dispose()
  expect(mock.listeners.size).toBe(0)
})
it('retries password login for the same peer and reports failure through local display state', async () => {
  const pairing = createPairing()
  await pairing.start({ peerId: 'peer', mode: 'MANAGE' })
  mock.listeners.get('pairingFailed')!({ deviceId: 'peer', error: 'password_required' })
  expect(pairing.state.value?.phase).toBe('PASSWORD_REQUIRED')
  await pairing.submit('fixture-password')
  expect(mock.native.mock.lastCall?.[0].variables).toEqual({ input: { peerId: 'peer', mode: 'MANAGE', password: 'fixture-password' } })
  mock.listeners.get('pairingFailed')!({ deviceId: 'peer', error: 'rejected' })
  expect(pairing.state.value?.error).toBe('rejected')
  pairing.dispose()
})
it('confirms using only the peer and decision', async () => {
  const pairing = createPairing()
  await pairing.start({ peerId: 'peer', mode: 'MANAGE' })
  mock.listeners.get('pairingRequested')!({ fromId: 'peer', verificationCode: '01234567', identityChanged: true })
  expect(pairing.state.value?.phase).toBe('TRUST_REQUIRED')
  await pairing.confirm('peer')
  expect(mock.native.mock.lastCall?.[0].variables).toEqual({ peerId: 'peer', accepted: true })
  expect(pairing.state.value?.phase).toBe('PAIRING')
  await pairing.cancel()
  expect(mock.native.mock.lastCall?.[0].variables).toEqual({ peerId: 'peer' })
  pairing.dispose()
})
it('cancels a peer directly without loading pairing snapshots', async () => {
  const pairing = createPairing()
  await pairing.cancel('restored')
  expect(mock.native).toHaveBeenCalledTimes(1)
  expect(mock.native.mock.lastCall?.[0].variables).toEqual({ peerId: 'restored' })
  pairing.dispose()
})
it('cancels by peer when a view closes during initiation', async () => {
  let resolve: (value: unknown) => void = () => {}
  mock.native.mockImplementationOnce(() => new Promise(done => { resolve = done }))
  const pairing = createPairing()
  const start = pairing.start({ peerId: 'peer', mode: 'MANAGE' })
  pairing.dispose()
  resolve({ data: { pairPeer: true } })
  await start
  expect(mock.native.mock.lastCall?.[0].variables).toEqual({ peerId: 'peer' })
})
