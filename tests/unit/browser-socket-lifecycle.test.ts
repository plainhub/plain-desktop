import { beforeEach, afterEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ sockets: [] as any[] }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: () => 'wss://example.test', getUrlToken: () => '' }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => 'token' }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => 'peer', getDesktopClientId: () => 'browser' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: () => new Uint8Array(32).fill(7) }))
vi.mock('@/lib/api/http', () => ({ openSocket: vi.fn(() => {
  const socket = { readyState: 1, send: vi.fn(), close: vi.fn() }
  mock.sockets.push(socket)
  return socket
}) }))

import { ScreenMirrorSocket } from '@/views/screen-mirror/screen-mirror-socket'
import { ImageEditorSocket } from '@/views/image-editor/sync/image-editor-socket'
import { openSocket } from '@/lib/api/http'
import { server, text } from '../helpers/websocket-server'

beforeEach(() => {
  vi.useFakeTimers()
  vi.stubGlobal('__IS_TAURI__', false)
  vi.stubGlobal('WebSocket', { OPEN: 1 })
  vi.clearAllMocks()
  mock.sockets.length = 0
  vi.spyOn(console, 'error').mockImplementation(() => {})
})
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals() })

for (const path of ['/screen', '/image_editor'] as const) {
  function fixture() {
    const ready = vi.fn(), update = vi.fn(), disconnected = vi.fn()
    const client = path === '/screen'
      ? new ScreenMirrorSocket({ onConnected: ready, onVideo: update, onAudio: update, onDisconnected: disconnected })
      : new ImageEditorSocket(ready, update)
    client.connect()
    const socket = mock.sockets.at(-1)!
    const event = path === '/screen' ? 'SCREEN_MIRROR_VIDEO' : 'IMAGE_EDITOR_UPDATE'
    function open() {
      socket.onopen()
      const peer = server(new Uint8Array(32).fill(7), socket.send.mock.calls[0][0], path, 'browser')
      socket.onmessage({ data: peer.ack.buffer })
      return peer
    }
    return { client, socket, ready, update, disconnected, event, open }
  }

  it(`${path} confirms at sequence one and isolates callbacks from protocol validation`, () => {
    const { client, socket, ready, update, event, open } = fixture()
    ready.mockImplementation(() => { throw new Error('UI failed') })
    update.mockImplementationOnce(() => { throw new Error('UI failed') })
    const peer = open()
    const confirmation = socket.send.mock.calls[1][0] as Uint8Array
    expect(new DataView(confirmation.buffer, confirmation.byteOffset).getBigUint64(0)).toBe(1n)
    expect(peer.decode(confirmation)).toEqual(new Uint8Array())
    let sequence = 1n
    if (path === '/image_editor') socket.onmessage({ data: peer.frame(sequence++, text('IMAGE_EDITOR_READY\0')).buffer })
    for (let i = 0; i < 2; i++) socket.onmessage({ data: peer.frame(sequence++, text(`${event}\0data`)).buffer })
    expect(ready).toHaveBeenCalledOnce()
    expect(update).toHaveBeenCalledTimes(2)
    expect(socket.close).not.toHaveBeenCalled()
    vi.advanceTimersByTime(10000)
    expect(socket.close).not.toHaveBeenCalled()
    client.disconnect()
    expect(vi.getTimerCount()).toBe(0)
  })

  it(`${path} times out once and ignores all frames while closing`, () => {
    const { client, socket, ready, update } = fixture()
    socket.onopen()
    const peer = server(new Uint8Array(32).fill(7), socket.send.mock.calls[0][0], path, 'browser')
    vi.advanceTimersByTime(9999)
    expect(socket.close).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)
    expect(socket.close).toHaveBeenCalledExactlyOnceWith(1008, 'handshake_timeout')
    socket.onmessage({ data: peer.ack.buffer })
    socket.onopen()
    socket.onerror()
    expect(socket.send).toHaveBeenCalledTimes(1)
    expect(ready).not.toHaveBeenCalled()
    expect(update).not.toHaveBeenCalled()
    expect(socket.close).toHaveBeenCalledTimes(1)
    client.disconnect()
    expect(vi.getTimerCount()).toBe(0)
  })

  it.each(['ack', 'registration', 'confirmation', 'frame', 'event'])(`${path} fails closed on invalid %s`, failure => {
    const { client, socket, ready, update, open } = fixture()
    if (failure === 'registration') {
      socket.send.mockImplementationOnce(() => { throw new Error('send failed') })
      socket.onopen()
    } else if (failure === 'frame' || failure === 'event') {
      const peer = open()
      socket.onmessage({ data: failure === 'frame' ? 'not binary' : peer.frame(1n, text('WRONG_EVENT\0')).buffer })
    } else {
      socket.onopen()
      const peer = server(new Uint8Array(32).fill(7), socket.send.mock.calls[0][0], path, 'browser')
      if (failure === 'confirmation') socket.send.mockImplementationOnce(() => { throw new Error('send failed') })
      socket.onmessage({ data: failure === 'ack' ? new ArrayBuffer(0) : peer.ack.buffer })
      expect(ready).not.toHaveBeenCalled()
    }
    expect(socket.close).toHaveBeenCalledExactlyOnceWith(1008, failure === 'registration' ? 'invalid_handshake' : 'invalid_event')
    expect(update).not.toHaveBeenCalled()
    expect(vi.getTimerCount()).toBe(0)
    client.disconnect()
  })

  it(`${path} reconnects with fresh state and ignores abandoned callbacks`, () => {
    const { client, socket, ready, disconnected, open } = fixture()
    open()
    ready.mockClear()
    socket.onclose()
    socket.onclose()
    if (path === '/image_editor') {
      vi.advanceTimersByTime(999)
      expect(mock.sockets).toHaveLength(1)
      vi.advanceTimersByTime(1)
    } else {
      expect(disconnected).toHaveBeenCalledOnce()
      client.connect()
    }
    expect(mock.sockets).toHaveLength(2)
    socket.onopen()
    socket.onmessage({ data: new ArrayBuffer(0) })
    socket.onerror()
    socket.onclose()
    const next = mock.sockets[1]
    next.onopen()
    const peer = server(new Uint8Array(32).fill(7), next.send.mock.calls[0][0], path, 'browser')
    next.onmessage({ data: peer.ack.buffer })
    if (path === '/image_editor') next.onmessage({ data: peer.frame(1n, text('IMAGE_EDITOR_READY\0')).buffer })
    expect(ready).toHaveBeenCalledOnce()
    client.disconnect()
    next.onclose()
    vi.advanceTimersByTime(20000)
    expect(mock.sockets).toHaveLength(2)
    expect(vi.getTimerCount()).toBe(0)
  })

  it(`${path} handles opening failures without retaining a socket`, () => {
    vi.mocked(openSocket).mockImplementationOnce(() => { throw new Error('open failed') })
    const { client, disconnected } = fixture()
    expect(mock.sockets).toHaveLength(0)
    if (path === '/image_editor') vi.advanceTimersByTime(1000)
    else {
      expect(disconnected).toHaveBeenCalledOnce()
      client.connect()
    }
    expect(mock.sockets).toHaveLength(1)
    client.disconnect()
  })
}
