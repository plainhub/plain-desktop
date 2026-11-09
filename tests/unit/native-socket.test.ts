import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ local: false, invoke: vi.fn(), scope: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: mock.invoke }))
vi.mock('@/lib/api/native-client', () => ({ nativeViewScope: mock.scope }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => mock.local }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => 'paired-peer', getDesktopClientId: vi.fn(() => { throw new Error('No native client credentials') }) }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: vi.fn(() => { throw new Error('No native token') }) }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: vi.fn(() => { throw new Error('No native URL') }), getUrlToken: vi.fn(() => { throw new Error('No native URL token') }) }))
vi.mock('@/lib/api/http', () => ({ openSocket: vi.fn(() => { throw new Error('No WebView socket') }) }))

import { openNativeSocket } from '@/lib/api/native-socket'
import { ScreenMirrorSocket } from '@/views/screen-mirror/screen-mirror-socket'
import { ImageEditorSocket } from '@/views/image-editor/sync/image-editor-socket'

function deferred<T>() { let resolve!: (value: T) => void; let reject!: (error: unknown) => void; const promise = new Promise<T>((a, b) => { resolve = a; reject = b }); return { promise, resolve, reject } }
async function flush() { for (let i = 0; i < 15; i++) await Promise.resolve() }
let reads: ReturnType<typeof deferred<ArrayBuffer>>[]
function frame(...bytes: number[]) { reads.shift()!.resolve(new Uint8Array(bytes).buffer) }
function callbacks() { return { opened: vi.fn(), message: vi.fn(), dropped: vi.fn() } }

beforeEach(() => {
  vi.stubGlobal('__IS_TAURI__', true)
  vi.spyOn(console, 'error').mockImplementation(() => {})
  mock.local = false
  reads = []
  mock.scope.mockReset().mockResolvedValue('view-generation')
  mock.invoke.mockReset().mockImplementation((command: string) => {
    if (command === 'native_socket_open') return Promise.resolve('scoped-handle')
    if (command === 'native_socket_read') { const read = deferred<ArrayBuffer>(); reads.push(read); return read.promise }
    return Promise.resolve()
  })
})
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.useRealTimers() })

it('uses only paired identity and scoped binary IPC for ordered native screen and controls', async () => {
  const events = { onConnected: vi.fn(), onDisconnected: vi.fn(), onVideo: vi.fn(), onAudio: vi.fn() }
  const socket = new ScreenMirrorSocket(events)
  socket.connect(); socket.connect()
  await flush()
  expect(mock.invoke).toHaveBeenCalledWith('native_socket_open', { kind: 'screen', scope: 'view-generation', target: { kind: 'peer', peerId: 'paired-peer' } })
  expect(mock.invoke.mock.calls.filter(([command]) => command === 'native_socket_open')).toHaveLength(1)
  expect(socket.sendBytes(new Uint8Array([1]))).toBe(false)
  frame(0); await flush()
  expect(events.onConnected).toHaveBeenCalledOnce()
  frame(1, 0, 255, 1); await flush()
  frame(2, 2, 0, 3); await flush()
  expect(events.onVideo).toHaveBeenCalledExactlyOnceWith(new Uint8Array([0, 255, 1]))
  expect(events.onAudio).toHaveBeenCalledExactlyOnceWith(new Uint8Array([2, 0, 3]))
  const payload = new Uint8Array([0x54, 1, 2, 3])
  expect(socket.sendBytes(payload)).toBe(true)
  payload[0] = 0
  await flush()
  expect(mock.invoke).toHaveBeenCalledWith('native_socket_send', new Uint8Array([0x54, 1, 2, 3]), { headers: { 'x-plain-native-scope': 'view-generation', 'x-plain-socket-handle': 'scoped-handle' } })
  socket.disconnect()
  frame(1, 8); await flush()
  expect(events.onVideo).toHaveBeenCalledTimes(1)
  expect(events.onDisconnected).not.toHaveBeenCalled()
  expect(mock.invoke).toHaveBeenCalledWith('native_socket_close', { scope: 'view-generation', handle: 'scoped-handle' })
})

it('waits for native editor readiness, retries a dropped session and stops on disconnect', async () => {
  vi.useFakeTimers()
  mock.local = true
  const ready = vi.fn(), update = vi.fn()
  const socket = new ImageEditorSocket(ready, update)
  socket.connect(); await flush()
  expect(mock.invoke).toHaveBeenCalledWith('native_socket_open', { kind: 'imageEditor', scope: 'view-generation', target: { kind: 'local' } })
  frame(0); await flush()
  expect(ready).not.toHaveBeenCalled()
  frame(3); await flush()
  frame(4, 0, 255); await flush()
  expect(ready).toHaveBeenCalledOnce()
  expect(update).toHaveBeenCalledExactlyOnceWith(new Uint8Array([0, 255]))
  reads.shift()!.reject(new Error('closed')); await flush()
  await vi.advanceTimersByTimeAsync(1000); await flush()
  expect(mock.invoke.mock.calls.filter(([command]) => command === 'native_socket_open')).toHaveLength(2)
  socket.disconnect()
  reads.shift()!.reject(new Error('closed')); await flush()
  await vi.advanceTimersByTimeAsync(2000)
  expect(mock.invoke.mock.calls.filter(([command]) => command === 'native_socket_open')).toHaveLength(2)
})

it('closes an abandoned open without publishing readiness or starting a read', async () => {
  const opening = deferred<string>()
  mock.invoke.mockImplementation(() => opening.promise)
  const events = callbacks()
  const socket = openNativeSocket('screen', events)
  await flush(); socket.close(); opening.resolve('late-handle'); await flush()
  expect(mock.invoke.mock.calls.map(([command]) => command)).toEqual(['native_socket_open', 'native_socket_close'])
  expect(events.opened).not.toHaveBeenCalled()
  expect(events.dropped).not.toHaveBeenCalled()
})

it('serializes control sends and fails closed instead of accumulating an unbounded queue', async () => {
  const sending = deferred<void>()
  const invoke = mock.invoke.getMockImplementation()!
  mock.invoke.mockImplementation((command, ...args) => command === 'native_socket_send' ? sending.promise : invoke(command, ...args))
  const events = callbacks()
  const socket = openNativeSocket('screen', events)
  await flush(); frame(0); await flush()
  for (let i = 0; i < 8; i++) expect(socket.send(new Uint8Array([i]))).toBe(true)
  await flush()
  expect(mock.invoke.mock.calls.filter(([command]) => command === 'native_socket_send')).toHaveLength(1)
  expect(socket.send(new Uint8Array([9]))).toBe(false)
  expect(events.dropped).toHaveBeenCalledOnce()
  sending.resolve(); await flush()
  expect(mock.invoke.mock.calls.filter(([command]) => command === 'native_socket_send')).toHaveLength(1)
})

it('rejects wrong kinds, duplicate readiness, malformed binary and oversized frames', async () => {
  for (const bytes of [new Uint8Array([3]).buffer, new Uint8Array([0]).buffer, new ArrayBuffer(0), new ArrayBuffer(16 * 1024 * 1024 + 1), [1, 2]]) {
    const events = callbacks(), socket = openNativeSocket('screen', events)
    await flush(); frame(0); await flush()
    reads.shift()!.resolve(bytes as ArrayBuffer); await flush()
    expect(events.message).not.toHaveBeenCalled()
    expect(events.dropped).toHaveBeenCalledOnce()
    socket.close()
  }
})
