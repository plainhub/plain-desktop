import { beforeEach, afterEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ sockets: [] as any[], token: 'fixture' }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: () => 'wss://example.test', getUrlToken: () => '' }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => mock.token }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => 'peer', getDesktopClientId: () => 'browser' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
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
import { server, text } from '../helpers/websocket-server'

const key = new Uint8Array(32).fill(7)
function fixture() {
  const callbacks = { onConnected: vi.fn(), onDisconnected: vi.fn(), onVideo: vi.fn(), onAudio: vi.fn() }
  const client = new ScreenMirrorSocket(callbacks)
  client.connect()
  const socket = mock.sockets.at(-1)!
  let peer: ReturnType<typeof server>
  const open = () => {
    socket.readyState = 1
    socket.onopen()
    peer = server(key, socket.send.mock.calls[0][0], '/screen', 'browser')
    socket.onmessage({ data: peer.ack.buffer })
    return peer
  }
  return { client, socket, callbacks, open }
}
beforeEach(() => {
  vi.useFakeTimers()
  vi.stubGlobal('__IS_TAURI__', false)
  vi.clearAllMocks()
  mock.sockets.length = 0
  mock.token = 'fixture'
  vi.spyOn(console, 'error').mockImplementation(() => {})
})
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals() })

it('opens once and waits for the authenticated acknowledgement before exposing controls', () => {
  const { client, socket, callbacks } = fixture()
  client.connect()
  expect(openSocket).toHaveBeenCalledExactlyOnceWith('wss://example.test/screen?cid=browser', 'peer')
  expect(socket.binaryType).toBe('arraybuffer')
  socket.onopen()
  expect(callbacks.onConnected).not.toHaveBeenCalled()
  const peer = server(key, socket.send.mock.calls[0][0], '/screen', 'browser')
  expect(peer.init).toEqual({ clientNonce: expect.stringMatching(/^[a-f0-9]{64}$/) })
  socket.onmessage({ data: peer.ack.buffer })
  expect(peer.decode(socket.send.mock.calls[1][0])).toEqual(new Uint8Array())
  expect(callbacks.onConnected).toHaveBeenCalledOnce()
})

it('routes authenticated binary video and audio with one ordered stream and rejects replay', () => {
  const { socket, callbacks, open } = fixture()
  const peer = open()
  const bytes = new Uint8Array([0, 255, 1])
  socket.onmessage({ data: peer.frame(1n, Buffer.concat([text('SCREEN_MIRROR_VIDEO\0'), bytes])).buffer })
  socket.onmessage({ data: peer.frame(2n, Buffer.concat([text('SCREEN_MIRROR_AUDIO\0'), bytes])).buffer })
  const last = peer.frame(2n, Buffer.concat([text('SCREEN_MIRROR_AUDIO\0'), bytes])).buffer
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
    const peer = open()
    const other = wrongConnection ? server(key, socket.send.mock.calls[0][0], '/screen', 'browser', 5) : peer
    socket.onmessage({ data: other.frame(1n, text(wrongConnection ? 'SCREEN_MIRROR_VIDEO\0data' : 'CONTENT_CHANGED\0data')).buffer })
    expect(socket.close).toHaveBeenCalledExactlyOnceWith(1008, 'invalid_event')
    expect(callbacks.onVideo).not.toHaveBeenCalled()
    client.disconnect()
  }
})

it('reports a missing session without creating a socket', () => {
  mock.token = ''
  const callbacks = { onConnected: vi.fn(), onDisconnected: vi.fn(), onVideo: vi.fn(), onAudio: vi.fn() }
  new ScreenMirrorSocket(callbacks).connect()
  expect(openSocket).not.toHaveBeenCalled()
  expect(callbacks.onDisconnected).toHaveBeenCalledOnce()
})

it('sends strictly ordered encrypted raw controls after the confirmation', () => {
  const { client, socket, open } = fixture()
  vi.stubGlobal('WebSocket', { OPEN: 1 })
  expect(client.sendBytes(new Uint8Array([1]))).toBe(false)
  const peer = open()
  const payload = new Uint8Array([0x54, 1, 2, 3])
  for (let index = 0; index < 3; index++) {
    expect(client.sendBytes(payload)).toBe(true)
    const frame = socket.send.mock.calls[index + 2][0] as Uint8Array
    expect(new DataView(frame.buffer, frame.byteOffset).getBigUint64(1)).toBe(BigInt(index + 2))
    expect(peer.decode(frame)).toEqual(payload)
  }
  client.disconnect()
  expect(client.sendBytes(payload)).toBe(false)
})

it('binds controls to the acknowledged connection after reconnecting', () => {
  vi.stubGlobal('WebSocket', { OPEN: 1 })
  const { client, open } = fixture()
  const oldPeer = open()
  client.disconnect()
  client.connect()
  const next = mock.sockets.at(-1)!
  next.readyState = 1
  next.onopen()
  const peer = server(key, next.send.mock.calls[0][0], '/screen', 'browser')
  next.onmessage({ data: peer.ack.buffer })
  client.sendBytes(new Uint8Array([1]))
  const control = next.send.mock.calls[2][0] as Uint8Array
  expect(peer.decode(control)).toEqual(new Uint8Array([1]))
  expect(() => oldPeer.decode(control)).toThrow()
  client.disconnect()
})

it('closes an unacknowledged handshake and clears the deadline when disconnected', () => {
  const { client, socket, callbacks } = fixture()
  socket.onopen()
  vi.advanceTimersByTime(10000)
  expect(socket.close).toHaveBeenCalledWith(1008, 'handshake_timeout')
  expect(callbacks.onConnected).not.toHaveBeenCalled()
  client.disconnect()
  client.connect()
  const next = mock.sockets.at(-1)!
  client.disconnect()
  vi.advanceTimersByTime(10000)
  expect(next.close).toHaveBeenCalledTimes(1)
})

it('drops controls after a send failure and never reuses its consumed sequence', () => {
  vi.stubGlobal('WebSocket', { OPEN: 1 })
  const { client, socket, open } = fixture()
  open()
  socket.send.mockImplementationOnce(() => { throw new Error('send failed') })
  expect(client.sendBytes(new Uint8Array([1]))).toBe(false)
  expect(client.sendBytes(new Uint8Array([2]))).toBe(false)
  expect(socket.close).toHaveBeenCalledWith(1008, 'invalid_control')
  client.disconnect()
})
