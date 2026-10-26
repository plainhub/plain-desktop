import { createHash } from 'node:crypto'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { beforeEach, afterEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ sockets: [] as any[], token: 'fixture' }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: () => 'wss://example.test', getLocalToken: () => '' }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => mock.token }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => 'peer' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/prefs', () => ({ get: () => 'browser' }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: () => new Uint8Array(32).fill(7) }))
vi.mock('@/lib/api/http', () => ({
  openSocket: vi.fn(() => {
    const socket = { readyState: 0, send: vi.fn(), close: vi.fn() }
    mock.sockets.push(socket)
    return socket
  }),
}))

import { ScreenMirrorSocket } from '../../src/views/screen-mirror/screen-mirror-socket'
import { openSocket } from '../../src/lib/api/http'
import { chachaDecrypt } from '../../src/lib/api/crypto'

const key = new Uint8Array(32).fill(7)
function frame(registration: Uint8Array, counter: bigint, name: string, data: Uint8Array): ArrayBuffer {
  const sequence = Buffer.alloc(8)
  sequence.writeBigUInt64BE(counter)
  const aad = Buffer.concat([Buffer.from('plain-ws-event-v1\0'), createHash('sha256').update(registration).digest(), sequence, Buffer.from(name)])
  const nonce = new Uint8Array(24).fill(Number(counter) + 1)
  const encrypted = xchacha20poly1305(key, nonce, aad).encrypt(data)
  return new Uint8Array(Buffer.concat([Buffer.from(name + '\0'), sequence, nonce, encrypted])).buffer
}
function fixture() {
  const callbacks = { onConnected: vi.fn(), onDisconnected: vi.fn(), onVideo: vi.fn(), onAudio: vi.fn() }
  const client = new ScreenMirrorSocket(callbacks)
  client.connect()
  const socket = mock.sockets.at(-1)!
  const open = () => {
    socket.readyState = 1
    socket.onopen()
  }
  return { client, socket, callbacks, open }
}
beforeEach(() => {
  vi.clearAllMocks()
  mock.sockets.length = 0
  mock.token = 'fixture'
  vi.spyOn(console, 'error').mockImplementation(() => {})
})
afterEach(() => vi.restoreAllMocks())

it('opens only one screen socket and registers with a fresh timestamp when it opens', () => {
  const { client, socket, callbacks, open } = fixture()
  client.connect()
  expect(openSocket).toHaveBeenCalledExactlyOnceWith('wss://example.test/screen?cid=browser', 'peer')
  expect(socket.binaryType).toBe('arraybuffer')
  expect(callbacks.onConnected).not.toHaveBeenCalled()
  const now = vi.spyOn(Date, 'now').mockReturnValue(1700000123456)
  open()
  expect(chachaDecrypt(key, socket.send.mock.calls[0][0])).toBe('1700000123456')
  expect(callbacks.onConnected).toHaveBeenCalledOnce()
  now.mockRestore()
})

it('routes authenticated binary video and audio with one ordered stream and rejects replay', () => {
  const { socket, callbacks, open } = fixture()
  open()
  const registration = socket.send.mock.calls[0][0]
  const bytes = new Uint8Array([0, 255, 1])
  socket.onmessage({ data: frame(registration, 0n, 'SCREEN_MIRROR_VIDEO', bytes) })
  socket.onmessage({ data: frame(registration, 1n, 'SCREEN_MIRROR_AUDIO', bytes) })
  const last = frame(registration, 1n, 'SCREEN_MIRROR_AUDIO', bytes)
  expect(callbacks.onVideo).toHaveBeenCalledExactlyOnceWith(bytes)
  expect(callbacks.onAudio).toHaveBeenCalledExactlyOnceWith(bytes)
  socket.onmessage({ data: last })
  expect(socket.close).toHaveBeenCalledExactlyOnceWith(1008, 'invalid_event')
  expect(callbacks.onAudio).toHaveBeenCalledTimes(1)
})

it('ignores late open, data and close callbacks after leaving or replacing a screen session', () => {
  const { client, socket, callbacks } = fixture()
  client.disconnect()
  socket.onopen()
  expect(socket.send).not.toHaveBeenCalled()
  client.connect()
  socket.onclose()
  socket.onmessage({ data: new ArrayBuffer(0) })
  expect(callbacks.onDisconnected).not.toHaveBeenCalled()
  expect(mock.sockets).toHaveLength(2)
  const current = mock.sockets[1]
  current.readyState = 1
  current.onopen()
  current.onclose()
  expect(callbacks.onDisconnected).toHaveBeenCalledOnce()
  client.connect()
  expect(mock.sockets).toHaveLength(3)
})

it('rejects wrong event kinds and data authenticated for a different connection', () => {
  for (const wrongConnection of [false, true]) {
    const { client, socket, callbacks, open } = fixture()
    open()
    const registration = wrongConnection ? new Uint8Array([1, 2, 3]) : socket.send.mock.calls[0][0]
    socket.onmessage({ data: frame(registration, 0n, wrongConnection ? 'SCREEN_MIRROR_VIDEO' : 'CONTENT_CHANGED', new Uint8Array([1])) })
    expect(socket.close).toHaveBeenCalledExactlyOnceWith(1008, 'invalid_event')
    expect(callbacks.onVideo).not.toHaveBeenCalled()
    }
})

it('reports a missing session without creating a socket', () => {
  mock.token = ''
  const callbacks = { onConnected: vi.fn(), onDisconnected: vi.fn(), onVideo: vi.fn(), onAudio: vi.fn() }
  new ScreenMirrorSocket(callbacks).connect()
  expect(openSocket).not.toHaveBeenCalled()
  expect(callbacks.onDisconnected).toHaveBeenCalledOnce()
})
