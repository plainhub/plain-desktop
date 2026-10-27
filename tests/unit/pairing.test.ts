import { beforeEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ native: vi.fn(), listeners: new Map<string, () => void>() }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: mock.native }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe: (name: string, fn: () => void) => { mock.listeners.set(name, fn); return () => mock.listeners.delete(name) },
  onAttached: (fn: () => void) => { mock.listeners.set('attached', fn); return () => mock.listeners.delete('attached') },
} }))
import { createPairing, type PairingState } from '../../src/lib/device/pairing'

const initial: PairingState = { deviceId: 'peer', generation: 'first', mode: 'MANAGE', phase: 'STARTING', error: null }
let snapshot: PairingState[]
beforeEach(() => {
  vi.resetAllMocks()
  mock.listeners.clear()
  snapshot = [initial]
  mock.native.mockImplementation(async ({ query }: { query: string }) => ({ data: query.includes('pairingStates') ? { pairingStates: snapshot } : { pairDevice: initial } }))
})

it('sends pairing intent and password and reconciles subscription updates', async () => {
  const pairing = createPairing()
  await pairing.start({ id: 'peer', mode: 'MANAGE' })
  await pairing.submit('fixture-password')
  expect(mock.native.mock.calls.find(([request]) => request.query.includes('pairDevice') && request.variables.input.password)?.[0].variables).toEqual({ input: { generation: 'first', password: 'fixture-password' } })
  snapshot = [{ ...initial, phase: 'SUCCEEDED' }]
  mock.listeners.get('pairingChanged')!()
  await vi.waitFor(() => expect(pairing.state.value?.phase).toBe('SUCCEEDED'))
  snapshot = [{ ...initial, phase: 'FAILED' }]
  mock.listeners.get('pairingChanged')!()
  await Promise.resolve()
  expect(pairing.state.value?.phase).toBe('SUCCEEDED')
  pairing.dispose()
  expect(mock.listeners.size).toBe(0)
})

it('reconciles an early completion after initiation and ignores other generations', async () => {
  snapshot = [{ ...initial, phase: 'WAITING_CONFIRMATION' }]
  const pairing = createPairing()
  await pairing.start({ host: 'example.test', mode: 'MANAGE' })
  await vi.waitFor(() => expect(pairing.state.value?.phase).toBe('WAITING_CONFIRMATION'))
  snapshot = [{ ...initial, generation: 'stale', phase: 'SUCCEEDED' }]
  mock.listeners.get('pairingChanged')!()
  await Promise.resolve()
  expect(pairing.state.value?.phase).toBe('WAITING_CONFIRMATION')
  await pairing.cancel()
  expect(mock.native.mock.lastCall?.[0].variables).toEqual({ generation: 'first' })
  pairing.dispose()
})

it('cancels the returned generation when the view closes before initiation returns', async () => {
  let resolve: (value: unknown) => void = () => {}
  mock.native.mockImplementationOnce(() => new Promise(done => { resolve = done }))
  const pairing = createPairing()
  const start = pairing.start({ id: 'peer', mode: 'MANAGE' })
  pairing.dispose()
  mock.native.mockResolvedValue({ data: { cancelPairing: true } })
  resolve({ data: { pairDevice: initial } })
  await start
  expect(mock.native.mock.lastCall?.[0].variables).toEqual({ generation: 'first' })
})
