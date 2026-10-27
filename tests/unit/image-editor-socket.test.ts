import { createHash } from 'node:crypto'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { afterEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ sockets: [] as any[] }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: () => 'wss://example.test', getLocalToken: () => '' }))
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

function frame(registration: Uint8Array, counter: bigint, name: string, data: Uint8Array): ArrayBuffer {
  const sequence = Buffer.alloc(8)
  sequence.writeBigUInt64BE(counter)
  const aad = Buffer.concat([Buffer.from('plain-ws-event-v1\0'), createHash('sha256').update(registration).digest(), sequence, Buffer.from(name)])
  const nonce = new Uint8Array(24).fill(Number(counter) + 1)
  const encrypted = xchacha20poly1305(new Uint8Array(32).fill(7), nonce, aad).encrypt(data)
  return new Uint8Array(Buffer.concat([Buffer.from(name + '\0'), sequence, nonce, encrypted])).buffer
}

afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); mock.sockets.length = 0 })

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
  socket.onmessage({ data: frame(registration, 0n, 'IMAGE_EDITOR_READY', new Uint8Array()) })
  expect(ready).toHaveBeenCalledOnce()
  const bytes = new Uint8Array([1, 2, 3])
  const event = frame(registration, 1n, 'IMAGE_EDITOR_UPDATE', bytes)
  socket.onmessage({ data: event })
  expect(update).toHaveBeenCalledExactlyOnceWith(bytes)
  socket.onmessage({ data: event })
  expect(socket.close).toHaveBeenCalledWith(1008, 'invalid_event')
  socket.onclose()
  vi.advanceTimersByTime(1000)
  expect(mock.sockets).toHaveLength(2)
  client.disconnect()
})
