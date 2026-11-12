import { readFileSync } from 'node:fs'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { sha256 } from '@noble/hashes/sha2.js'

const mock = vi.hoisted(() => ({ socket: vi.fn(), agent: vi.fn(), base: vi.fn(), opaque: vi.fn() }))
vi.mock('@/lib/opaque/client', () => ({ startOpaqueLogin: mock.opaque }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: mock.base }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => Buffer.alloc(32, 5).toString('base64') }))
vi.mock('@/lib/api/http', () => ({ openSocket: mock.socket }))
vi.mock('@/lib/agent/agent', () => ({ getAccurateAgent: mock.agent }))
vi.mock('@/lib/api/time-sync', () => ({ getSyncedTimestamp: () => 1700000000000 }))
vi.mock('@/lib/api/crypto', async (original) => ({
  ...(await original<object>()),
  generateECDHKeyPair: vi.fn(),
}))
import { generateECDHKeyPair } from '../../src/lib/api/crypto'
import { openLoginSocket } from '../../src/lib/api/login-socket'
import { performLoginHandshake } from '../../src/lib/api/login-handshake'

const vector = JSON.parse(
  readFileSync(
    new URL(
      '../../../plain-core/plain-rs/testdata/crypto/web-approval-vector.json',
      import.meta.url
    ),
    'utf8'
  )
)
const bytes = (value: string) => new Uint8Array(Buffer.from(value, 'base64'))

class Socket {
  readyState = 0
  onopen?: () => void
  onmessage?: (event: { data: string | Blob }) => Promise<void>
  onclose?: (event: { reason: string }) => void
  onerror?: () => void
  send = vi.fn()
  close = vi.fn((_code?: number, reason = '') => {
    this.readyState = 3
    this.onclose?.({ reason })
  })
  async open() {
    this.readyState = 1
    this.onopen?.()
    await vi.advanceTimersByTimeAsync(0)
  }
  async message(data: string | Blob) {
    await this.onmessage?.({ data })
    await vi.advanceTimersByTimeAsync(0)
  }
}

let socket: Socket
beforeEach(() => {
  vi.useFakeTimers()
  vi.clearAllMocks()
  socket = new Socket()
  mock.socket.mockReturnValue(socket)
  mock.base.mockReturnValue('wss://example.invalid')
  mock.agent.mockResolvedValue({
    browser: { name: 'Fixture', version: '1' },
    os: { name: 'Fixture', version: '1' },
    isMobile: false,
  })
  vi.stubGlobal('window', {
    location: { protocol: 'https:', hostname: 'example.invalid' },
    setTimeout,
    clearTimeout,
  })
  vi.mocked(generateECDHKeyPair).mockReset().mockImplementation(() => ({
    secretKey: bytes(vector.clientPrivate), publicKey: bytes(vector.request.publicKey),
  }))
})
afterEach(() => {
  expect(mock.opaque).not.toHaveBeenCalled()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

function frame(key: Uint8Array, data: object) {
  const nonce = new Uint8Array(24).fill(9)
  const cipher = xchacha20poly1305(key, nonce, new Uint8Array([...new TextEncoder().encode('auth-response'), ...Buffer.from(vector.fingerprint, 'hex')])).encrypt(
    new TextEncoder().encode(JSON.stringify(data))
  )
  return new Blob([new Uint8Array([...nonce, ...cipher])])
}

function completed() {
  return { clientId: 'fixture-device', status: 'COMPLETED', chatPaired: false }
}

const params = () => ({
  passwordRequired: false,
  clientId: vector.clientId,
  initSignaturePublicKey: vector.serverIdentity,
  expectedDeviceId: 'fixture-device',
})

it.each(['http:', 'https:'])(
  'completes encrypted approval login over %s without WebCrypto subtle',
  async (protocol) => {
    vi.stubGlobal('window', {
      location: { protocol, hostname: 'example.invalid' },
      setTimeout,
      clearTimeout,
    })
    mock.base.mockReturnValue(
      protocol === 'http:' ? 'ws://example.invalid' : 'wss://example.invalid'
    )
    vi.stubGlobal('crypto', { getRandomValues: crypto.getRandomValues.bind(crypto) })
    const pending = vi.fn()
    const promise = performLoginHandshake({ ...params(), onPending: pending })
    await socket.open()
    expect(mock.socket.mock.calls[0][0]).toContain('/auth?')
    expect(new URL(mock.socket.mock.calls[0][0]).searchParams.has('auth')).toBe(false)
    expect(JSON.parse(socket.send.mock.calls[0][0])).toEqual(vector.request)
    await socket.message(JSON.stringify(vector.response))
    const request = new Uint8Array(socket.send.mock.calls[1][0])
    const decoded = JSON.parse(
      new TextDecoder().decode(
        xchacha20poly1305(bytes(vector.key), request.slice(0, 24), new Uint8Array([...new TextEncoder().encode('auth-request'), ...Buffer.from(vector.fingerprint, 'hex')])).decrypt(request.slice(24))
      )
    )
    expect(decoded).toEqual({
      appName: 'Fixture',
      appVersion: '1',
      osName: 'Fixture',
      osVersion: '1',
    })
    expect(decoded).not.toHaveProperty('password')
    await socket.message(frame(bytes(vector.key), { status: 'PENDING' }))
    expect(pending).toHaveBeenCalledExactlyOnceWith(vector.fingerprint)
    expect(socket.close).not.toHaveBeenCalled()
    await socket.message(frame(bytes(vector.key), completed()))
    await expect(promise).resolves.toMatchObject({
      clientId: 'fixture-device',
      token: vector.key,
      signaturePublicKey: vector.serverIdentity,
    })
    expect(vi.getTimerCount()).toBe(0)
  }
)

it('rejects a malformed or substituted hello without sending credentials', async () => {
  const promise = performLoginHandshake(params())
  const rejection = expect(promise).rejects.toBe('signature_verification_failed')
  await socket.open()
  await socket.message(
    JSON.stringify({ ...vector.response, signature: Buffer.alloc(64, 4).toString('base64') })
  )
  await rejection
  expect(socket.send).toHaveBeenCalledTimes(1)
  expect(vi.getTimerCount()).toBe(0)
})

it('does not replace a stored identity', async () => {
  await expect(
    performLoginHandshake({ ...params(), storedSignaturePublicKey: 'different-key' })
  ).rejects.toBe('peer_identity_changed')
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
  await socket.open()
  await vi.advanceTimersByTimeAsync(300000)
  await rejection
  expect(vi.getTimerCount()).toBe(0)
})

it('does not extend the login deadline after pending replies', async () => {
  const promise = performLoginHandshake(params())
  const rejection = expect(promise).rejects.toBe('login_expired')
  await socket.open()
  await socket.message(JSON.stringify(vector.response))
  await vi.advanceTimersByTimeAsync(299000)
  await socket.message(frame(bytes(vector.key), { status: 'PENDING' }))
  await vi.advanceTimersByTimeAsync(1000)
  await rejection
  expect(socket.send).toHaveBeenCalledTimes(2)
  expect(vi.getTimerCount()).toBe(0)
})

it('does not send credentials when cancelled during client metadata lookup', async () => {
  let finishAgent: (agent: unknown) => void = () => {}
  mock.agent.mockReturnValueOnce(
    new Promise((resolve) => {
      finishAgent = resolve
    })
  )
  const controller = new AbortController()
  const promise = performLoginHandshake({
    ...params(),
    signal: controller.signal,
  })
  const rejection = expect(promise).rejects.toBe('cancelled')
  await socket.open()
  await socket.message(JSON.stringify(vector.response))
  controller.abort()
  await rejection
  finishAgent({ browser: {}, os: {}, isMobile: false })
  await vi.advanceTimersByTimeAsync(0)
  await rejection
  expect(socket.send).toHaveBeenCalledTimes(1)
  expect(vi.getTimerCount()).toBe(0)
})

it('authenticates to A before delivering B login frames on the existing proxy URL', async () => {
  const path = '/proxy/device-b/auth?cid=fixture-browser'
  const url = `wss://example.invalid${path}`
  const opening = openLoginSocket(url)
  await socket.open()
  const init = socket.send.mock.calls[0][0] as Uint8Array
  const utf8 = new TextEncoder()
  const fields = (values: Uint8Array[]) => Buffer.concat(values.flatMap(value => {
    const length = Buffer.alloc(4)
    length.writeUInt32BE(value.length)
    return [length, value]
  }))
  const key = new Uint8Array(32).fill(5)
  const initAad = fields(['plain-ws-init', path, 'fixture-browser'].map(value => utf8.encode(value)))
  expect(JSON.parse(new TextDecoder().decode(xchacha20poly1305(key, init.subarray(0, 24), initAad).decrypt(init.subarray(24))))).toEqual({ clientNonce: expect.stringMatching(/^[a-f0-9]{64}$/) })
  const ackAad = fields([...['plain-ws-ack', path, 'fixture-browser'].map(value => utf8.encode(value)), sha256(init)])
  const nonce = new Uint8Array(24).fill(9)
  const ack = xchacha20poly1305(key, nonce, ackAad).encrypt(utf8.encode(JSON.stringify({ serverNonce: 'a'.repeat(64) })))
  await socket.message(new Blob([new Uint8Array([...nonce, ...ack])]))
  const connection = await opening
  expect(socket.send).toHaveBeenCalledTimes(2)
  const confirm = socket.send.mock.calls[1][0] as Uint8Array
  expect(new DataView(confirm.buffer, confirm.byteOffset).getBigUint64(0)).toBe(1n)
  const binding = sha256(fields([init, new Uint8Array([...nonce, ...ack])]))
  const confirmAad = fields([
    ...['plain-ws-frame', path, 'fixture-browser'].map(value => utf8.encode(value)),
    binding, utf8.encode('client'), confirm.subarray(0, 8),
  ])
  expect(xchacha20poly1305(key, confirm.subarray(8, 32), confirmAad).decrypt(confirm.subarray(32))).toEqual(new Uint8Array())
  connection.send('client hello')
  await socket.message('signed B hello')
  expect(await connection.receiveText()).toBe('signed B hello')
  expect(mock.socket).toHaveBeenCalledWith(url)
  connection.close()
})

it('keeps the A session handshake on the current API origin', async () => {
  await expect(openLoginSocket('wss://other.invalid/proxy/device-b/auth?cid=fixture-browser'))
    .rejects.toBe('proxy_auth_failed')
  expect(mock.socket).not.toHaveBeenCalled()
})
