import { readFileSync } from 'node:fs'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { p256 } from '@noble/curves/nist.js'
import { ed25519 } from '@noble/curves/ed25519.js'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { sha256 } from '@noble/hashes/sha2.js'
import { sha512Hex } from '../../src/lib/api/sha512'

const mock = vi.hoisted(() => ({ socket: vi.fn(), agent: vi.fn(), base: vi.fn() }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: mock.base }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => Buffer.alloc(32, 5).toString('base64') }))
vi.mock('@/lib/api/http', () => ({ openSocket: mock.socket }))
vi.mock('@/lib/agent/agent', () => ({ getAccurateAgent: mock.agent }))
vi.mock('@/lib/api/time-sync', () => ({ getSyncedTimestamp: () => 1000 }))
vi.mock('@/lib/api/crypto', async (original) => ({
  ...(await original<object>()),
  generateECDHKeyPair: vi.fn(),
}))
import { computeECDHSharedKey, generateECDHKeyPair } from '../../src/lib/api/crypto'
import { openLoginSocket } from '../../src/lib/api/login-socket'
import { performLoginHandshake } from '../../src/lib/api/login-handshake'

const vector = JSON.parse(
  readFileSync(
    new URL(
      '../../../plain-core/plain-rs/testdata/crypto/web-login-channel-vector.json',
      import.meta.url
    ),
    'utf8'
  )
)
const passwordVector = JSON.parse(
  readFileSync(
    new URL(
      '../../../plain-core/plain-rs/testdata/crypto/web-password-login-vector.json',
      import.meta.url
    ),
    'utf8'
  )
)
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
  vi.mocked(generateECDHKeyPair)
    .mockReset()
    .mockReturnValueOnce({ secretKey: sessionPrivate, publicKey: sessionPublic })
    .mockReturnValueOnce({
      secretKey: bytes(vector.clientPrivate),
      publicKey: bytes(vector.reveal.publicKey),
    })
  vi.spyOn(crypto, 'getRandomValues').mockImplementationOnce((target) => {
    new Uint8Array(target!.buffer).fill(3)
    return target
  })
})
afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

function frame(key: Uint8Array, data: object) {
  const nonce = new Uint8Array(24).fill(9)
  const cipher = xchacha20poly1305(key, nonce).encrypt(
    new TextEncoder().encode(JSON.stringify(data))
  )
  return new Blob([new Uint8Array([...nonce, ...cipher])])
}

function completed(context?: string) {
  const response = {
    clientId: 'fixture-device',
    status: 'COMPLETED',
    ecdhPublicKey: vector.hello.serverPublicKey,
    timestamp: 1000,
    chatPaired: false,
    ...(context ? { loginContext: context } : {}),
  }
  const signed = `fixture-device|COMPLETED|${response.ecdhPublicKey}|1000${context ? `|${context}` : ''}`
  return {
    ...response,
    signature: Buffer.from(
      ed25519.sign(new TextEncoder().encode(signed), new Uint8Array(32).fill(7))
    ).toString('base64'),
  }
}

const params = () => ({
  passwordRequired: false,
  clientId: vector.hello.clientId,
  initSignaturePublicKey: vector.signingPublicKey,
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
    expect(mock.socket.mock.calls[0][0]).toContain('auth=2')
    expect(JSON.parse(socket.send.mock.calls[0][0])).toEqual(vector.clientHello)
    await socket.message(JSON.stringify(vector.hello))
    expect(JSON.parse(socket.send.mock.calls[1][0])).toEqual(vector.reveal)
    const request = new Uint8Array(socket.send.mock.calls[2][0])
    const decoded = JSON.parse(
      new TextDecoder().decode(
        xchacha20poly1305(bytes(vector.requestKey), request.slice(0, 24)).decrypt(request.slice(24))
      )
    )
    expect(decoded).toMatchObject({
      loginContext: vector.context,
      appName: 'Fixture',
      appVersion: '1',
      ecdhPublicKey: Buffer.from(sessionPublic).toString('base64'),
    })
    expect(decoded).not.toHaveProperty('password')
    await socket.message(frame(bytes(vector.responseKey), { status: 'PENDING' }))
    expect(pending).toHaveBeenCalledExactlyOnceWith(expect.stringMatching(/^[0-9]{4} [0-9]{4}$/))
    expect(socket.close).not.toHaveBeenCalled()
    await socket.message(frame(bytes(vector.responseKey), completed(vector.context)))
    await expect(promise).resolves.toMatchObject({
      clientId: 'fixture-device',
      token: computeECDHSharedKey(sessionPrivate, bytes(vector.hello.serverPublicKey)),
      signaturePublicKey: vector.signingPublicKey,
    })
    expect(vi.getTimerCount()).toBe(0)
  }
)

it.each(['http:', 'https:'])(
  'sends only the password digest through the signed temporary channel over %s',
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
    const digest = sha512Hex('fixture-password')
    const passwordKey = new TextEncoder().encode(digest.slice(0, 32))
    const pending = vi.fn()
    const promise = performLoginHandshake({
      ...params(),
      passwordRequired: true,
      passwordHash: digest,
      onPending: pending,
    })
    await socket.open()
    expect(mock.socket.mock.calls[0][0]).toContain('auth=1')
    expect(JSON.parse(socket.send.mock.calls[0][0])).toEqual(passwordVector.clientHello)
    expect(socket.send).toHaveBeenCalledTimes(1)

    await socket.message(JSON.stringify(passwordVector.hello))
    expect(JSON.parse(socket.send.mock.calls[1][0])).toEqual(passwordVector.reveal)
    const encrypted = new Uint8Array(socket.send.mock.calls[2][0])
    expect(() =>
      xchacha20poly1305(passwordKey, encrypted.slice(0, 24)).decrypt(encrypted.slice(24))
    ).toThrow()
    const plaintext = xchacha20poly1305(
      bytes(passwordVector.requestKey),
      encrypted.slice(0, 24)
    ).decrypt(encrypted.slice(24))
    expect(JSON.parse(new TextDecoder().decode(plaintext))).toMatchObject({
      password: digest,
      loginContext: passwordVector.context,
      ecdhPublicKey: Buffer.from(sessionPublic).toString('base64'),
    })

    expect(new TextDecoder().decode(plaintext)).not.toContain('fixture-password')
    await socket.message(frame(bytes(passwordVector.responseKey), { status: 'PENDING' }))
    expect(pending).toHaveBeenCalledExactlyOnceWith(undefined)
    await socket.message(frame(bytes(passwordVector.responseKey), completed(passwordVector.context)))
    await expect(promise).resolves.toMatchObject({
      token: computeECDHSharedKey(sessionPrivate, bytes(vector.hello.serverPublicKey)),
    })
    expect(vi.getTimerCount()).toBe(0)
  }
)

it.each(['signature', 'clientCommitment'])(
  'does not send password credentials after a substituted %s',
  async (field) => {
    const promise = performLoginHandshake({
      ...params(),
      passwordRequired: true,
      passwordHash: passwordVector.passwordHash,
    })
    const rejection = expect(promise).rejects.toBeDefined()
    await socket.open()
    await socket.message(JSON.stringify({ ...passwordVector.hello, [field]: 'substituted' }))
    await rejection
    expect(socket.send).toHaveBeenCalledTimes(1)
    expect(vi.getTimerCount()).toBe(0)
  }
)

it('rejects a password response missing the outer encryption', async () => {
  const promise = performLoginHandshake({
    ...params(),
    passwordRequired: true,
    passwordHash: passwordVector.passwordHash,
  })
  const rejection = expect(promise).rejects.toBe('invalid_auth_response')
  await socket.open()
  await socket.message(JSON.stringify(passwordVector.hello))
  const key = new TextEncoder().encode(passwordVector.passwordHash.slice(0, 32))
  await socket.message(frame(key, completed(passwordVector.context)))
  await rejection
  expect(vi.getTimerCount()).toBe(0)
})

it('rejects a malformed or substituted hello without sending credentials', async () => {
  const promise = performLoginHandshake(params())
  const rejection = expect(promise).rejects.toBe('invalid_auth_response')
  await socket.open()
  await socket.message(
    JSON.stringify({ ...vector.hello, clientCommitment: Buffer.alloc(32, 4).toString('base64') })
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

it.each([false, true])(
  'does not extend the login deadline after pending replies, password=%s',
  async (passwordRequired) => {
    const promise = performLoginHandshake({
      ...params(),
      passwordRequired,
      passwordHash: passwordRequired ? passwordVector.passwordHash : undefined,
    })
    const rejection = expect(promise).rejects.toBe('login_expired')
    await socket.open()
    const challenge = passwordRequired ? passwordVector.hello : vector.hello
    await socket.message(JSON.stringify(challenge))
    await vi.advanceTimersByTimeAsync(299000)
    const pending = passwordRequired
      ? frame(bytes(passwordVector.responseKey), { status: 'PENDING' })
      : frame(bytes(vector.responseKey), { status: 'PENDING' })
    await socket.message(pending)
    await vi.advanceTimersByTimeAsync(1000)
    await rejection
    expect(socket.send).toHaveBeenCalledTimes(3)
    expect(vi.getTimerCount()).toBe(0)
  }
)

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
    passwordRequired: true,
    passwordHash: passwordVector.passwordHash,
    signal: controller.signal,
  })
  const rejection = expect(promise).rejects.toBe('cancelled')
  await socket.open()
  await socket.message(JSON.stringify(passwordVector.hello))
  controller.abort()
  await rejection
  finishAgent({ browser: {}, os: {}, isMobile: false })
  await vi.advanceTimersByTimeAsync(0)
  await rejection
  expect(socket.send).toHaveBeenCalledTimes(2)
  expect(vi.getTimerCount()).toBe(0)
})

it('authenticates to A before delivering B login frames on the existing proxy URL', async () => {
  const path = '/proxy/device-b/auth?cid=fixture-browser&auth=1'
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
  const initAad = fields(['plain-graphql-init-v1', path, 'fixture-browser'].map(value => utf8.encode(value)))
  expect(JSON.parse(new TextDecoder().decode(xchacha20poly1305(key, init.subarray(1, 25), initAad).decrypt(init.subarray(25))))).toMatchObject({ timestampMillis: 1000 })
  const ackAad = fields([...['plain-graphql-ack-v1', path, 'fixture-browser'].map(value => utf8.encode(value)), sha256(init)])
  const nonce = new Uint8Array(24).fill(9)
  const ack = xchacha20poly1305(key, nonce, ackAad).encrypt(utf8.encode(JSON.stringify({ serverNonce: 'a'.repeat(64) })))
  await socket.message(new Blob([new Uint8Array([1, ...nonce, ...ack])]))
  const connection = await opening
  connection.send('client hello')
  await socket.message('signed B hello')
  expect(await connection.receiveText()).toBe('signed B hello')
  expect(mock.socket).toHaveBeenCalledWith(url)
  connection.close()
})

it('keeps the A session handshake on the current API origin', async () => {
  await expect(openLoginSocket('wss://other.invalid/proxy/device-b/auth?cid=fixture-browser&auth=1'))
    .rejects.toBe('proxy_auth_failed')
  expect(mock.socket).not.toHaveBeenCalled()
})
