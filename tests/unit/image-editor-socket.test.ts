import { beforeEach, afterEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ sockets: [] as any[] }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: () => 'wss://example.test', getUrlToken: () => '' }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => 'token' }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => 'peer', getDesktopClientId: () => 'browser' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: () => new Uint8Array(32).fill(7) }))
vi.mock('@/lib/api/http', () => ({ openSocket: vi.fn(() => {
  const socket = { send: vi.fn(), close: vi.fn() }
  mock.sockets.push(socket)
  return socket
}) }))

import { ImageEditorSocket } from '../../src/views/image-editor/sync/image-editor-socket'
import { openSocket } from '../../src/lib/api/http'
import { server, text } from '../helpers/websocket-server'

beforeEach(() => vi.stubGlobal('__IS_TAURI__', false))
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); vi.useRealTimers(); mock.sockets.length = 0 })

it('waits for authenticated readiness, receives updates, rejects replay and reconnects', () => {
  vi.useFakeTimers()
  vi.spyOn(console, 'error').mockImplementation(() => {})
  const ready = vi.fn()
  const update = vi.fn()
  const client = new ImageEditorSocket(ready, update)
  client.connect()
  const socket = mock.sockets[0]
  expect(openSocket).toHaveBeenCalledWith('wss://example.test/image_editor?cid=browser', 'peer')
  socket.onopen()
  expect(ready).not.toHaveBeenCalled()
  const registration = socket.send.mock.calls[0][0]
  const key = new Uint8Array(32).fill(7)
  const peer = server(key, registration, '/image_editor', 'browser')
  expect(peer.init).toEqual({ clientNonce: expect.stringMatching(/^[a-f0-9]{64}$/) })
  socket.onmessage({ data: peer.ack.buffer })
  expect(peer.decode(socket.send.mock.calls[1][0])).toEqual(new Uint8Array())
  expect(ready).not.toHaveBeenCalled()
  socket.onmessage({ data: peer.frame(1n, text('IMAGE_EDITOR_READY\0')).buffer })
  expect(ready).toHaveBeenCalledOnce()
  const bytes = new Uint8Array([1, 2, 3])
  const event = peer.frame(2n, Buffer.concat([text('IMAGE_EDITOR_UPDATE\0'), bytes])).buffer
  socket.onmessage({ data: event })
  expect(update).toHaveBeenCalledExactlyOnceWith(bytes)
  socket.onmessage({ data: event })
  expect(socket.close).toHaveBeenCalledWith(1008, 'invalid_event')
  socket.onclose()
  vi.advanceTimersByTime(1000)
  expect(mock.sockets).toHaveLength(2)
  client.disconnect()
})
