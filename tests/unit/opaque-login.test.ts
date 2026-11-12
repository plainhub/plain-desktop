import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { hkdf } from '@noble/hashes/hkdf.js'
import { sha256 } from '@noble/hashes/sha2.js'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'

const mock = vi.hoisted(() => ({ start: vi.fn(), socket: vi.fn(), agent: vi.fn(), ecdh: vi.fn() }))
vi.mock('@/lib/opaque/client', () => ({ startOpaqueLogin: mock.start }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: () => 'ws://example.invalid' }))
vi.mock('@/lib/api/time-sync', () => ({ getSyncedTimestamp: () => 1000 }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => '' }))
vi.mock('@/lib/api/http', () => ({ openSocket: mock.socket }))
vi.mock('@/lib/agent/agent', () => ({ getAccurateAgent: mock.agent }))
vi.mock('@/lib/api/crypto', async original => ({ ...await original<object>(), generateECDHKeyPair: mock.ecdh }))
import { performLoginHandshake } from '../../src/lib/api/login-handshake'

const signingKey = new Uint8Array(32).fill(7)
const opaqueKey = new Uint8Array(32).fill(8)
const sessionKey = new Uint8Array(64).fill(9)
const ke1 = new Uint8Array(96).fill(1)
const ke2 = new Uint8Array(320).fill(2)
const ke3 = new Uint8Array(64).fill(3)
const b64 = (bytes: Uint8Array) => Buffer.from(bytes).toString('base64')
const utf8 = new TextEncoder()
const context = Buffer.concat([utf8.encode('LOGIN_PASSWORD'), utf8.encode('browser'), signingKey].flatMap(field => {
  const length = Buffer.alloc(4); length.writeUInt32BE(field.length)
  return [length, field]
}))
const hash = sha256(context)
const key = hkdf(sha256, sessionKey, hash, utf8.encode('api'), 32)
const aad = (response: boolean) => new Uint8Array([...utf8.encode(response ? 'AUTH_RESPONSE' : 'AUTH_REQUEST'), ...hash])
const params = () => ({ passwordRequired: true, password: 'synthetic password', clientId: 'browser', initSignaturePublicKey: b64(signingKey), initOpaquePublicKey: b64(opaqueKey), expectedDeviceId: 'device' })

class Socket {
  onopen?: () => void
  onmessage?: (event: { data: ArrayBuffer | string }) => void
  onclose?: (event: { reason: string }) => void
  onerror?: () => void
  send = vi.fn()
  close = vi.fn((_code?: number, reason = '') => this.onclose?.({ reason }))
  async open() { this.onopen?.(); await vi.advanceTimersByTimeAsync(0) }
  async message(data: Uint8Array | string) {
    this.onmessage?.({ data: typeof data === 'string' ? data : new Uint8Array(data).buffer })
    await vi.advanceTimersByTimeAsync(0)
  }
}
let socket: Socket
let state: { request: ReturnType<typeof vi.fn>; finish: ReturnType<typeof vi.fn>; free: ReturnType<typeof vi.fn> }
let result: { ke3: Uint8Array; apiKey: Uint8Array; requestAad: Uint8Array; responseAad: Uint8Array; serverPublicKey: Uint8Array; free: ReturnType<typeof vi.fn> }
async function begin(extra = {}) {
  const promise = performLoginHandshake({ ...params(), ...extra })
  await vi.dynamicImportSettled()
  await vi.advanceTimersByTimeAsync(0)
  await socket.open()
  return { promise }
}
function reply(value: object) {
  const nonce = new Uint8Array(24).fill(4)
  return new Uint8Array([...nonce, ...xchacha20poly1305(key, nonce, aad(true)).encrypt(utf8.encode(JSON.stringify(value)))])
}
beforeEach(() => {
  vi.useFakeTimers()
  vi.clearAllMocks()
  vi.stubGlobal('window', { setTimeout, clearTimeout })
  socket = new Socket()
  mock.socket.mockReturnValue(socket)
  result = { ke3, apiKey: key.slice(), requestAad: aad(false), responseAad: aad(true), serverPublicKey: opaqueKey, free: vi.fn() }
  state = { request: vi.fn(() => ke1), finish: vi.fn(() => result), free: vi.fn() }
  mock.start.mockResolvedValue(state)
  mock.agent.mockResolvedValue({ browser: { name: 'Fixture', version: '1' }, os: { name: 'Fixture', version: '2' }, isMobile: false })
})
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.useRealTimers() })

it('exchanges only binary OPAQUE frames and encrypted metadata, without password or extra ECDH', async () => {
  const { promise } = await begin()
  expect(mock.start).toHaveBeenCalledWith(params().password, undefined)
  expect(mock.ecdh).not.toHaveBeenCalled()
  expect(new URL(mock.socket.mock.calls[0][0]).search).toBe('?cid=browser')
  expect(socket.send.mock.calls[0][0]).toEqual(ke1)
  await socket.message(ke2)
  expect(state.finish).toHaveBeenCalledWith(ke2, 'browser', signingKey)
  const request = socket.send.mock.calls[1][0] as Uint8Array
  expect(request.slice(0, 64)).toEqual(ke3)
  const body = JSON.parse(new TextDecoder().decode(xchacha20poly1305(key, request.slice(64, 88), aad(false)).decrypt(request.slice(88))))
  expect(body).toEqual({ appName: 'Fixture', appVersion: '1', osName: 'Fixture', osVersion: '2' })
  await socket.message(reply({ status: 'COMPLETED', clientId: 'device', chatPaired: false }))
  await expect(promise).resolves.toEqual({ clientId: 'device', token: b64(key), signaturePublicKey: b64(signingKey), opaquePublicKey: b64(opaqueKey), chatPaired: false })
  expect(result.apiKey.every(value => value === 0)).toBe(true)
  expect(state.free).not.toHaveBeenCalled()
  expect(vi.getTimerCount()).toBe(0)
})

it.each(['wrong password', 'substituted KE2'])('rejects %s without sending KE3 or metadata', async () => {
  state.finish.mockRejectedValue('invalid_password')
  const { promise } = await begin()
  const rejected = expect(promise).rejects.toBe('invalid_password')
  await socket.message(ke2)
  await rejected
  expect(socket.send).toHaveBeenCalledTimes(1)
  expect(state.free).not.toHaveBeenCalled()
})

it('requires the authenticated OPAQUE key to match init before sending metadata', async () => {
  result.serverPublicKey = new Uint8Array(32).fill(5)
  const { promise } = await begin()
  const rejected = expect(promise).rejects.toBe('peer_identity_changed')
  await socket.message(ke2)
  await rejected
  expect(socket.send).toHaveBeenCalledTimes(1)
})

it('rejects changed stored OPAQUE pins before starting a connection', async () => {
  await expect(performLoginHandshake({ ...params(), storedOpaquePublicKey: b64(new Uint8Array(32).fill(4)) })).rejects.toBe('peer_identity_changed')
  expect(mock.start).not.toHaveBeenCalled()
  expect(mock.socket).not.toHaveBeenCalled()
})

it('frees pending WASM state on cancellation', async () => {
  const controller = new AbortController()
  const { promise } = await begin({ signal: controller.signal })
  const rejected = expect(promise).rejects.toBe('cancelled')
  controller.abort()
  await rejected
  expect(state.free).toHaveBeenCalledOnce()
  expect(socket.send).toHaveBeenCalledTimes(1)
  expect(vi.getTimerCount()).toBe(0)
})

it('does not send metadata when cancelled during metadata lookup', async () => {
  let resolve: (value: object) => void = () => {}
  mock.agent.mockReturnValue(new Promise(done => { resolve = done }))
  const controller = new AbortController()
  const { promise } = await begin({ signal: controller.signal })
  const rejected = expect(promise).rejects.toBe('cancelled')
  await socket.message(ke2)
  controller.abort()
  await rejected
  resolve({ browser: {}, os: {}, isMobile: false })
  await vi.advanceTimersByTimeAsync(0)
  expect(socket.send).toHaveBeenCalledTimes(1)
})

it('rejects configuration changes instead of falling back to device confirmation', async () => {
  const { promise } = await begin()
  const rejected = expect(promise).rejects.toBe('invalid_login_request')
  socket.onclose?.({ reason: 'invalid_login_request' })
  await rejected
  expect(mock.ecdh).not.toHaveBeenCalled()
  expect(state.free).toHaveBeenCalledOnce()
})

it('requires the result to be encrypted and bound to this login', async () => {
  const { promise } = await begin()
  const rejected = expect(promise).rejects.toBe('invalid_auth_response')
  await socket.message(ke2)
  await socket.message(utf8.encode(JSON.stringify({ status: 'COMPLETED', clientId: 'device' })))
  await rejected
})
