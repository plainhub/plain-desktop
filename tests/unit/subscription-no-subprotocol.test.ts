import { createHash } from 'node:crypto'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { afterEach, expect, it, vi } from 'vitest'

const key = Uint8Array.from({ length: 32 }, (_, index) => index + 1)
const mock = vi.hoisted(() => ({
  socket: {
    protocol: '', send: vi.fn(), close: vi.fn(), binaryType: '',
    onopen: undefined as undefined | (() => void),
    onmessage: undefined as undefined | ((event: MessageEvent) => void),
  },
  openSocket: vi.fn(),
}))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: () => 'wss://device.example:8443' }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => 'fixture' }))
vi.mock('@/lib/device/client-id', () => ({ getDesktopClientId: () => 'fixture-client', getRemoteClientId: () => 'fixture-device' }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: () => key }))
vi.mock('@/lib/api/time-sync', () => ({ getSyncedTimestamp: () => 123456 }))
vi.mock('@/lib/api/http', () => ({ openSocket: mock.openSocket }))
import { openSubscriptionConnection } from '../../src/lib/api/subscription-connection'

function fields(...parts: Uint8Array[]): Buffer {
  return Buffer.concat(parts.map(part => {
    const length = Buffer.alloc(4)
    length.writeUInt32BE(part.length)
    return Buffer.concat([length, part])
  }))
}
const text = (value: string) => Buffer.from(value, 'utf8')
const hash = (value: Uint8Array) => createHash('sha256').update(value).digest()
function serverFrame(prefix: Uint8Array, plain: unknown, aad: Uint8Array): ArrayBuffer {
  const nonce = Uint8Array.from({ length: 24 }, (_, index) => index + prefix.length)
  const encrypted = xchacha20poly1305(key, nonce, aad).encrypt(text(JSON.stringify(plain)))
  return Uint8Array.from(Buffer.concat([prefix, nonce, encrypted])).buffer
}

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.clearAllMocks() })

it('keeps the encrypted handshake and messages when no subprotocol is negotiated', () => {
  vi.useFakeTimers()
  vi.stubGlobal('__IS_TAURI__', false)
  mock.openSocket.mockReturnValue(mock.socket)
  const callbacks = { opened: vi.fn(), message: vi.fn(), dropped: vi.fn() }
  const connection = openSubscriptionConnection(callbacks)
  expect(mock.openSocket).toHaveBeenCalledWith('wss://device.example:8443/graphql?cid=fixture-client', 'fixture-device')
  expect(mock.socket.binaryType).toBe('arraybuffer')
  mock.socket.onopen!()
  expect(callbacks.dropped).not.toHaveBeenCalled()
  expect(callbacks.opened).not.toHaveBeenCalled()
  const init = mock.socket.send.mock.calls[0]![0] as Uint8Array
  expect(init[0]).toBe(1)
  const initAad = fields(text('plain-graphql-init-v1'), text('/graphql'), text('fixture-client'))
  const plain = xchacha20poly1305(key, init.subarray(1, 25), initAad).decrypt(init.subarray(25))
  const registration = JSON.parse(new TextDecoder().decode(plain))
  expect(registration.timestampMillis).toBe(123456)
  expect(registration.clientNonce).toMatch(/^[a-f0-9]{64}$/)
  const ackAad = fields(text('plain-graphql-ack-v1'), text('/graphql'), text('fixture-client'), hash(init))
  const ack = serverFrame(Uint8Array.of(1), { serverNonce: '42'.repeat(32) }, ackAad)
  mock.socket.onmessage!({ data: ack } as MessageEvent)
  expect(callbacks.opened).toHaveBeenCalledExactlyOnceWith()
  const binding = hash(fields(init, new Uint8Array(ack)))
  const message = { type: 'subscribe', id: 'fixture', payload: { query: 'subscription { preferencesChanged }' } }
  connection.send(message)
  const frame = mock.socket.send.mock.calls[1]![0] as Uint8Array
  expect(frame[0]).toBe(1)
  expect(Buffer.from(frame.subarray(1, 9)).readBigUInt64BE()).toBe(0n)
  const outgoingAad = fields(text('plain-graphql-frame-v1'), text('/graphql'), text('fixture-client'), binding, text('client'), frame.subarray(1, 9))
  const outgoing = xchacha20poly1305(key, frame.subarray(9, 33), outgoingAad).decrypt(frame.subarray(33))
  expect(JSON.parse(new TextDecoder().decode(outgoing))).toEqual(message)
  const prefix = Buffer.alloc(9)
  prefix[0] = 1
  const incomingAad = fields(text('plain-graphql-frame-v1'), text('/graphql'), text('fixture-client'), binding, text('server'), prefix.subarray(1))
  const event = { type: 'next', id: 'fixture', payload: { data: { preferencesChanged: true } } }
  mock.socket.onmessage!({ data: serverFrame(prefix, event, incomingAad) } as MessageEvent)
  expect(callbacks.message).toHaveBeenCalledExactlyOnceWith(event)
  expect(callbacks.dropped).not.toHaveBeenCalled()
  connection.close()
  expect(mock.socket.close).toHaveBeenCalledOnce()
  expect(vi.getTimerCount()).toBe(0)
})
