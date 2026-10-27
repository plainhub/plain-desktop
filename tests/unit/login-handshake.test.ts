import { readFileSync } from 'node:fs'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { p256 } from '@noble/curves/nist.js'
import { ed25519 } from '@noble/curves/ed25519.js'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { sha512Hex } from '../../src/lib/api/sha512'

const mock = vi.hoisted(() => ({ socket: vi.fn(), agent: vi.fn(), base: vi.fn() }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: mock.base }))
vi.mock('@/lib/api/http', () => ({ openSocket: mock.socket }))
vi.mock('@/lib/agent/agent', () => ({ getAccurateAgent: mock.agent }))
vi.mock('@/lib/api/time-sync', () => ({ getSyncedTimestamp: () => 1000 }))
vi.mock('@/lib/api/crypto', async (original) => ({ ...await original<object>(), generateECDHKeyPair: vi.fn() }))
import { computeECDHSharedKey, generateECDHKeyPair } from '../../src/lib/api/crypto'
import { performLoginHandshake } from '../../src/lib/api/login-handshake'

const vector = JSON.parse(readFileSync(new URL('../../../plain-core/plain-rs/testdata/crypto/web-login-channel-vector.json', import.meta.url), 'utf8'))
const bytes = (value: string) => new Uint8Array(Buffer.from(value, 'base64'))
const sessionPrivate = new Uint8Array(32).fill(6)
const sessionPublic = p256.getPublicKey(sessionPrivate, false)

class Socket {
  readyState = 0
  onopen?: () => void
  onmessage?: (event: { data: string | Blob }) => Promise<void>
  onclose?: (event: { reason: string }) => void
  onerror?: () => void
  send = vi.fn()
  close = vi.fn((_code?: number, reason = '') => { this.readyState = 3; this.onclose?.({ reason }) })
  open() { this.readyState = 1; this.onopen?.() }
  async message(data: string | Blob) { await this.onmessage?.({ data }) }
}

let socket: Socket
beforeEach(() => {
  vi.useFakeTimers()
  vi.clearAllMocks()
  socket = new Socket()
  mock.socket.mockReturnValue(socket)
  mock.base.mockReturnValue('wss://example.invalid')
  mock.agent.mockResolvedValue({ browser: { name: 'Fixture', version: '1' }, os: { name: 'Fixture', version: '1' }, isMobile: false })
  vi.stubGlobal('window', { location: { protocol: 'https:', hostname: 'example.invalid' }, setTimeout, clearTimeout })
  vi.mocked(generateECDHKeyPair).mockReset()
    .mockReturnValueOnce({ secretKey: sessionPrivate, publicKey: sessionPublic })
    .mockReturnValueOnce({ secretKey: bytes(vector.clientPrivate), publicKey: bytes(vector.reveal.publicKey) })
  vi.spyOn(crypto, 'getRandomValues').mockImplementationOnce((target) => { new Uint8Array(target!.buffer).fill(3); return target })
})
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.useRealTimers() })

function frame(key: Uint8Array, data: object) {
  const nonce = new Uint8Array(24).fill(9)
  const cipher = xchacha20poly1305(key, nonce).encrypt(new TextEncoder().encode(JSON.stringify(data)))
  return new Blob([new Uint8Array([...nonce, ...cipher])])
}

function completed(context?: string) {
  const response = { clientId: 'fixture-device', status: 'COMPLETED', ecdhPublicKey: vector.hello.serverPublicKey, timestamp: 1000, chatPaired: false, ...(context ? { loginContext: context } : {}) }
  const signed = `fixture-device|COMPLETED|${response.ecdhPublicKey}|1000${context ? `|${context}` : ''}`
  return { ...response, signature: Buffer.from(ed25519.sign(new TextEncoder().encode(signed), new Uint8Array(32).fill(7))).toString('base64') }
}

const params = () => ({ passwordRequired: false, clientId: vector.hello.clientId, initSignaturePublicKey: vector.signingPublicKey, expectedDeviceId: 'fixture-device' })

it.each(['http:', 'https:'])('completes encrypted approval login over %s without WebCrypto subtle', async (protocol) => {
  vi.stubGlobal('window', { location: { protocol, hostname: 'example.invalid' }, setTimeout, clearTimeout })
  mock.base.mockReturnValue(protocol === 'http:' ? 'ws://example.invalid' : 'wss://example.invalid')
  vi.stubGlobal('crypto', { getRandomValues: crypto.getRandomValues.bind(crypto) })
  const pending = vi.fn()
  const promise = performLoginHandshake({ ...params(), onPending: pending })
  socket.open()
  expect(mock.socket.mock.calls[0][0]).toContain('/auth?')
  expect(mock.socket.mock.calls[0][0]).toContain('auth=2')
  expect(JSON.parse(socket.send.mock.calls[0][0])).toEqual(vector.clientHello)
  await socket.message(JSON.stringify(vector.hello))
  expect(JSON.parse(socket.send.mock.calls[1][0])).toEqual(vector.reveal)
  const request = new Uint8Array(socket.send.mock.calls[2][0])
  const decoded = JSON.parse(new TextDecoder().decode(xchacha20poly1305(bytes(vector.requestKey), request.slice(0, 24)).decrypt(request.slice(24))))
  expect(decoded).toMatchObject({ loginContext: vector.context, browserName: 'Fixture', ecdhPublicKey: Buffer.from(sessionPublic).toString('base64') })
  expect(decoded).not.toHaveProperty('password')
  await socket.message(frame(bytes(vector.responseKey), { status: 'PENDING' }))
  expect(pending).toHaveBeenCalledExactlyOnceWith(expect.stringMatching(/^[0-9]{4} [0-9]{4}$/))
  expect(socket.close).not.toHaveBeenCalled()
  await socket.message(frame(bytes(vector.responseKey), completed(vector.context)))
  await expect(promise).resolves.toMatchObject({ clientId: 'fixture-device', token: computeECDHSharedKey(sessionPrivate, bytes(vector.hello.serverPublicKey)), signaturePublicKey: vector.signingPublicKey })
  expect(vi.getTimerCount()).toBe(0)
})

it.each(['http:', 'https:'])('completes fixed-password login over %s without WebCrypto subtle', async (protocol) => {
  vi.stubGlobal('window', { location: { protocol, hostname: 'example.invalid' }, setTimeout, clearTimeout })
  mock.base.mockReturnValue(protocol === 'http:' ? 'ws://example.invalid' : 'wss://example.invalid')
  vi.stubGlobal('crypto', { getRandomValues: crypto.getRandomValues.bind(crypto) })
  const digest = sha512Hex('fixture-password')
  const key = new TextEncoder().encode(digest.slice(0, 32))
  const promise = performLoginHandshake({ ...params(), passwordRequired: true, passwordHash: digest })
  socket.open()
  await Promise.resolve()
  expect(mock.socket.mock.calls[0][0]).toContain('/auth?')
  expect(mock.socket.mock.calls[0][0]).toContain('auth=1')
  const request = new Uint8Array(socket.send.mock.calls[0][0])
  expect(JSON.parse(new TextDecoder().decode(xchacha20poly1305(key, request.slice(0, 24)).decrypt(request.slice(24)))).password).toBe(digest)
  await socket.message(frame(key, completed()))
  await expect(promise).resolves.toMatchObject({ token: computeECDHSharedKey(sessionPrivate, bytes(vector.hello.serverPublicKey)) })
})

it('rejects a malformed or substituted hello without sending credentials', async () => {
  const promise = performLoginHandshake(params())
  const rejection = expect(promise).rejects.toBe('invalid_auth_response')
  socket.open()
  await socket.message(JSON.stringify({ ...vector.hello, clientCommitment: Buffer.alloc(32, 4).toString('base64') }))
  await rejection
  expect(socket.send).toHaveBeenCalledTimes(1)
  expect(vi.getTimerCount()).toBe(0)
})

it('does not replace a stored identity', async () => {
  await expect(performLoginHandshake({ ...params(), storedSignaturePublicKey: 'different-key' })).rejects.toBe('peer_identity_changed')
  expect(mock.socket).not.toHaveBeenCalled()
})

it('cancels a pending connection and clears both deadlines', async () => {
  const controller = new AbortController()
  const promise = performLoginHandshake({ ...params(), signal: controller.signal })
  const rejection = expect(promise).rejects.toBe('cancelled')
  controller.abort()
  await rejection
  expect(socket.close).toHaveBeenCalledWith(3001, 'cancelled')
  expect(vi.getTimerCount()).toBe(0)
})

it('bounds the complete handshake after connection instead of timing only socket opening', async () => {
  const promise = performLoginHandshake(params())
  const rejection = expect(promise).rejects.toBe('login_expired')
  socket.open()
  await vi.advanceTimersByTimeAsync(300000)
  await rejection
  expect(vi.getTimerCount()).toBe(0)
})
