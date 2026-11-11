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
import { server } from '../helpers/websocket-server'
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
  const initAad = fields(text('plain-ws-init'), text('/graphql'), text('fixture-client'))
  const plain = xchacha20poly1305(key, init.subarray(1, 25), initAad).decrypt(init.subarray(25))
  const registration = JSON.parse(new TextDecoder().decode(plain))
  expect(Object.keys(registration)).toEqual(['clientNonce'])
  expect(registration.clientNonce).toMatch(/^[a-f0-9]{64}$/)
  const ackAad = fields(text('plain-ws-ack'), text('/graphql'), text('fixture-client'), hash(init))
  const ack = serverFrame(Uint8Array.of(1), { serverNonce: '42'.repeat(32) }, ackAad)
  mock.socket.onmessage!({ data: ack } as MessageEvent)
  expect(callbacks.opened).toHaveBeenCalledExactlyOnceWith()
  const binding = hash(fields(init, new Uint8Array(ack)))
  const message = { type: 'subscribe', id: 'fixture', payload: { query: 'subscription { preferencesChanged }' } }
  connection.send(message)
  const frame = mock.socket.send.mock.calls[2]![0] as Uint8Array
  expect(frame[0]).toBe(1)
  expect(Buffer.from(frame.subarray(1, 9)).readBigUInt64BE()).toBe(2n)
  const outgoingAad = fields(text('plain-ws-frame'), text('/graphql'), text('fixture-client'), binding, text('client'), frame.subarray(1, 9))
  const outgoing = xchacha20poly1305(key, frame.subarray(9, 33), outgoingAad).decrypt(frame.subarray(33))
  expect(JSON.parse(new TextDecoder().decode(outgoing))).toEqual(message)
  const prefix = Buffer.alloc(9)
  prefix[0] = 1
  prefix.writeBigUInt64BE(1n, 1)
  const incomingAad = fields(text('plain-ws-frame'), text('/graphql'), text('fixture-client'), binding, text('server'), prefix.subarray(1))
  const event = { type: 'next', id: 'fixture', payload: { data: { preferencesChanged: true } } }
  mock.socket.onmessage!({ data: serverFrame(prefix, event, incomingAad) } as MessageEvent)
  expect(callbacks.message).toHaveBeenCalledExactlyOnceWith(event)
  expect(callbacks.dropped).not.toHaveBeenCalled()
  connection.close()
  expect(mock.socket.close).toHaveBeenCalledOnce()
  expect(vi.getTimerCount()).toBe(0)
})

it('does not retain heartbeat timers when opening callbacks close the connection', () => {
  vi.useFakeTimers()
  vi.stubGlobal('__IS_TAURI__', false)
  mock.openSocket.mockReturnValue(mock.socket)
  const connection = openSubscriptionConnection({ opened: () => connection.close(), message: vi.fn(), dropped: vi.fn() })
  mock.socket.onopen!()
  const init = mock.socket.send.mock.calls[0]![0] as Uint8Array
  const ackAad = fields(text('plain-ws-ack'), text('/graphql'), text('fixture-client'), hash(init))
  mock.socket.onmessage!({ data: serverFrame(Uint8Array.of(1), { serverNonce: '42'.repeat(32) }, ackAad) } as MessageEvent)
  expect(vi.getTimerCount()).toBe(0)
})

it('closes on a failed heartbeat send without starting a pong deadline', () => {
  vi.useFakeTimers()
  vi.stubGlobal('__IS_TAURI__', false)
  mock.openSocket.mockReturnValue(mock.socket)
  const dropped = vi.fn()
  openSubscriptionConnection({ opened: vi.fn(), message: vi.fn(), dropped })
  mock.socket.onopen!()
  const init = mock.socket.send.mock.calls[0]![0] as Uint8Array
  const ackAad = fields(text('plain-ws-ack'), text('/graphql'), text('fixture-client'), hash(init))
  mock.socket.onmessage!({ data: serverFrame(Uint8Array.of(1), { serverNonce: '42'.repeat(32) }, ackAad) } as MessageEvent)
  mock.socket.send.mockImplementationOnce(() => { throw new Error('send failed') })
  vi.advanceTimersByTime(20000)
  expect(dropped).toHaveBeenCalledOnce()
  expect(vi.getTimerCount()).toBe(0)
})

it('confirms immediately before opened even with no operations and a delayed first subscription', () => {
  vi.useFakeTimers()
  vi.stubGlobal('__IS_TAURI__', false)
  mock.openSocket.mockReturnValue(mock.socket)
  const callbacks = {
    opened: vi.fn(() => expect(mock.socket.send).toHaveBeenCalledTimes(2)),
    message: vi.fn(), dropped: vi.fn(),
  }
  const connection = openSubscriptionConnection(callbacks)
  mock.socket.onopen!()
  const peer = server(key, mock.socket.send.mock.calls[0]![0], '/graphql', 'fixture-client')
  mock.socket.onmessage!({ data: peer.ack.buffer } as MessageEvent)
  expect(callbacks.dropped).not.toHaveBeenCalled()
  expect(callbacks.opened).toHaveBeenCalledOnce()
  const confirm = mock.socket.send.mock.calls[1]![0] as Uint8Array
  expect(new DataView(confirm.buffer, confirm.byteOffset).getBigUint64(1)).toBe(1n)
  expect(JSON.parse(new TextDecoder().decode(peer.decode(confirm)))).toEqual({ type: 'ping' })
  vi.advanceTimersByTime(15000)
  expect(mock.socket.send).toHaveBeenCalledTimes(2)
  const message = { type: 'subscribe', id: 'late', payload: { query: 'subscription { preferencesChanged }' } }
  connection.send(message)
  const operation = mock.socket.send.mock.calls[2]![0] as Uint8Array
  expect(new DataView(operation.buffer, operation.byteOffset).getBigUint64(1)).toBe(2n)
  expect(JSON.parse(new TextDecoder().decode(peer.decode(operation)))).toEqual(message)
  expect(callbacks.dropped).not.toHaveBeenCalled()
  connection.close()
})

it('does not expose an opened connection when its initial confirmation send fails', () => {
  vi.useFakeTimers()
  vi.stubGlobal('__IS_TAURI__', false)
  mock.openSocket.mockReturnValue(mock.socket)
  const callbacks = { opened: vi.fn(), message: vi.fn(), dropped: vi.fn() }
  const connection = openSubscriptionConnection(callbacks)
  mock.socket.onopen!()
  const peer = server(key, mock.socket.send.mock.calls[0]![0], '/graphql', 'fixture-client')
  mock.socket.send.mockImplementationOnce(() => { throw new Error('confirmation send failed') })
  mock.socket.onmessage!({ data: peer.ack.buffer } as MessageEvent)
  expect(callbacks.opened).not.toHaveBeenCalled()
  expect(callbacks.dropped).toHaveBeenCalledExactlyOnceWith(new Error('confirmation send failed'))
  expect(mock.socket.close).toHaveBeenCalledOnce()
  expect(vi.getTimerCount()).toBe(0)
  connection.send({ type: 'ping' })
  expect(mock.socket.send).toHaveBeenCalledTimes(2)
})
