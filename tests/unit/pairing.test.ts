import { beforeEach, expect, it, vi } from 'vitest'
import { randomBytes } from 'node:crypto'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'

const mock = vi.hoisted(() => ({ native: vi.fn(), close: vi.fn(), frame: undefined as undefined | ((frame: ArrayBuffer) => void) }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: mock.native }))
vi.mock('@/lib/api/native-events', () => ({ subscribeNativeEvents: (frame: (frame: ArrayBuffer) => void) => { mock.frame = frame; return { close: mock.close } } }))
vi.mock('@/lib/api/api', () => ({ getLocalToken: () => 'local' }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: () => new Uint8Array(32) }))
import { createPairing, type PairingState } from '../../src/lib/device/pairing'

const initial: PairingState = { deviceId: 'peer', generation: 'first', mode: 'MANAGE', phase: 'STARTING', error: null }
function encoded(state: PairingState, aad: Uint8Array | undefined = Buffer.from('plain-event-v1\0PAIRING_STATE_CHANGED')) {
  const nonce = randomBytes(24)
  const cipher = xchacha20poly1305(new Uint8Array(32), nonce, aad).encrypt(Buffer.from(JSON.stringify(state)))
  return new Uint8Array(Buffer.concat([Buffer.from('PAIRING_STATE_CHANGED\0'), nonce, cipher])).buffer
}
function send(state: PairingState) { mock.frame?.(encoded(state)) }
beforeEach(() => { vi.resetAllMocks() })

it('sends intent and password continuation without accepting frontend derived credentials', async () => {
  mock.native.mockResolvedValue({ data: { pairDevice: initial } })
  const pairing = createPairing()
  await pairing.start({ id: 'peer', mode: 'MANAGE' })
  await pairing.submit('fixture-password')
  expect(mock.native.mock.calls[0][0].variables).toEqual({ input: { id: 'peer', mode: 'MANAGE' } })
  expect(mock.native.mock.calls[1][0].variables).toEqual({ input: { generation: 'first', password: 'fixture-password' } })
  send({ ...initial, phase: 'SUCCEEDED' })
  send({ ...initial, phase: 'FAILED' })
  expect(pairing.state.value?.phase).toBe('SUCCEEDED')
  pairing.dispose()
  expect(mock.close).toHaveBeenCalledOnce()
  expect(mock.native).toHaveBeenCalledTimes(2)
})

it('keeps an early completion and ignores another generation', async () => {
  mock.native.mockImplementation(async () => {
    send({ ...initial, phase: 'WAITING_CONFIRMATION' })
    return { data: { pairDevice: initial } }
  })
  const pairing = createPairing()
  await pairing.start({ host: 'example.test', mode: 'MANAGE' })
  expect(pairing.state.value?.phase).toBe('WAITING_CONFIRMATION')
  send({ ...initial, generation: 'stale', phase: 'SUCCEEDED' })
  expect(pairing.state.value?.phase).toBe('WAITING_CONFIRMATION')
  mock.native.mockResolvedValue({ data: { cancelPairing: true } })
  await pairing.cancel()
  expect(mock.native.mock.lastCall?.[0].variables).toEqual({ generation: 'first' })
  send({ ...initial, phase: 'CANCELLED' })
  pairing.dispose()
})

it('cancels the returned generation when the view closes before initiation returns', async () => {
  let resolve: (value: unknown) => void = () => {}
  mock.native.mockImplementationOnce(() => new Promise((done) => { resolve = done }))
  const pairing = createPairing()
  const start = pairing.start({ id: 'peer', mode: 'MANAGE' })
  pairing.dispose()
  mock.native.mockResolvedValue({ data: { cancelPairing: true } })
  resolve({ data: { pairDevice: initial } })
  await start
  expect(mock.native.mock.lastCall?.[0].variables).toEqual({ generation: 'first' })
})

it('rejects an old unbound or renamed native pairing event before changing the projection', async () => {
  mock.native.mockResolvedValue({ data: { pairDevice: initial } })
  const pairing = createPairing()
  await pairing.start({ id: 'peer', mode: 'MANAGE' })
  const complete = { ...initial, phase: 'SUCCEEDED' }
  expect(() => mock.frame?.(encoded(complete, new Uint8Array()))).toThrow()
  expect(() => mock.frame?.(encoded(complete, Buffer.from('plain-event-v1\0CONTENT_CHANGED')))).toThrow()
  expect(pairing.state.value?.phase).toBe('STARTING')
  send(complete)
  expect(pairing.state.value?.phase).toBe('SUCCEEDED')
  pairing.dispose()
})
