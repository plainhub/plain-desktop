import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { openNative, openSubscriptionConnection } from '@/lib/api/subscription-connection'
import { SubscriptionClient } from '@/lib/api/subscription-client'

const mock = vi.hoisted(() => ({ local: true, remote: 'remote', invoke: vi.fn(), scope: vi.fn(), socket: { close: vi.fn() }, openSocket: vi.fn() }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => mock.local }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: mock.invoke }))
vi.mock('@/lib/api/native-client', () => ({ nativeViewScope: mock.scope }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: () => 'wss://remote.example:8443' }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => 'session' }))
vi.mock('@/lib/device/client-id', () => ({ getDesktopClientId: () => 'self', getRemoteClientId: () => mock.remote }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: vi.fn() }))
vi.mock('@/lib/api/time-sync', () => ({ getSyncedTimestamp: () => 0 }))
vi.mock('@/lib/api/http', () => ({ openSocket: mock.openSocket }))

function callbacks() { return { opened: vi.fn(), message: vi.fn(), dropped: vi.fn() } }
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(done => { resolve = done })
  return { promise, resolve }
}

beforeEach(() => {
  vi.resetAllMocks()
  vi.useFakeTimers()
  vi.stubGlobal('__IS_TAURI__', true)
  mock.local = true
  mock.remote = 'remote'
  mock.scope.mockResolvedValue('document')
  mock.openSocket.mockReturnValue(mock.socket)
})
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals() })

it.each([undefined, 'third-peer'])('preserves browser encrypted subscription routing for proxy target %s', (peerId) => {
  vi.stubGlobal('__IS_TAURI__', false)
  mock.local = false
  const connection = openSubscriptionConnection(callbacks(), peerId)
  const path = peerId ? '/proxy/third-peer/graphql' : '/graphql'
  expect(mock.openSocket).toHaveBeenCalledExactlyOnceWith(`wss://remote.example:8443${path}?cid=self`, 'remote', 'plain-graphql-ws-v1')
  expect(mock.invoke).not.toHaveBeenCalled()
  expect(mock.scope).not.toHaveBeenCalled()
  connection.close()
  expect(mock.socket.close).toHaveBeenCalledTimes(1)
  expect(vi.getTimerCount()).toBe(0)
})

it.each([true, false])('opens Desktop local=%s subscriptions with the window scope and explicit target', async (local) => {
  mock.local = local
  const read = deferred<string>()
  mock.invoke.mockImplementation((command: string) => command === 'native_subscription_open'
    ? Promise.resolve(7)
    : command === 'native_subscription_read' ? read.promise : Promise.resolve())
  const events = callbacks()
  const connection = openSubscriptionConnection(events)
  await vi.waitFor(() => expect(events.opened).toHaveBeenCalledTimes(1))
  expect(mock.scope).toHaveBeenCalledTimes(1)
  expect(mock.invoke).toHaveBeenCalledWith('native_subscription_open', { scope: 'document', target: local ? { kind: 'local' } : { kind: 'peer', peerId: 'remote' } })
  expect(mock.openSocket).not.toHaveBeenCalled()
  connection.send({ type: 'ping' })
  await vi.waitFor(() => expect(mock.invoke).toHaveBeenCalledWith('native_subscription_send', { handle: 7, message: '{"type":"ping"}' }))
  connection.close()
  connection.close()
  read.resolve('{"type":"next","id":"old"}')
  await Promise.resolve()
  expect(mock.invoke.mock.calls.filter(([command]) => command === 'native_subscription_close')).toEqual([['native_subscription_close', { handle: 7 }]])
  expect(events.message).not.toHaveBeenCalled()
  expect(events.dropped).not.toHaveBeenCalled()
})

it.each(['third-peer', ''])('rejects explicit Desktop proxy targets without network fallback: %s', (peerId) => {
  expect(() => openSubscriptionConnection(callbacks(), peerId)).toThrow('Native peer-proxy subscriptions are unsupported')
  expect(mock.invoke).not.toHaveBeenCalled()
  expect(mock.scope).not.toHaveBeenCalled()
  expect(mock.openSocket).not.toHaveBeenCalled()
})

it('rejects a missing selected remote ID without substituting the local identity', () => {
  mock.local = false
  mock.remote = ''
  expect(() => openSubscriptionConnection(callbacks())).toThrow('native_peer_id_missing')
  expect(mock.invoke).not.toHaveBeenCalled()
  expect(mock.scope).not.toHaveBeenCalled()
  expect(mock.openSocket).not.toHaveBeenCalled()
})

it('does not allocate a subscription if the connection closes during the scope handshake', async () => {
  const scope = deferred<string>()
  mock.scope.mockReturnValue(scope.promise)
  const events = callbacks()
  const connection = openSubscriptionConnection(events)
  connection.close()
  scope.resolve('document')
  await Promise.resolve()
  expect(mock.invoke).not.toHaveBeenCalled()
  expect(events.opened).not.toHaveBeenCalled()
  expect(events.dropped).not.toHaveBeenCalled()
})

it('closes only a late old handle after a replacement opens on another peer', async () => {
  mock.local = false
  const oldOpen = deferred<number>()
  const read = deferred<string>()
  let opens = 0
  mock.invoke.mockImplementation((command: string) => command === 'native_subscription_open'
    ? ++opens === 1 ? oldOpen.promise : Promise.resolve(8)
    : command === 'native_subscription_read' ? read.promise : Promise.resolve())
  const oldEvents = callbacks()
  const old = openSubscriptionConnection(oldEvents)
  await vi.waitFor(() => expect(opens).toBe(1))
  old.close()
  mock.remote = 'replacement'
  const newEvents = callbacks()
  const current = openSubscriptionConnection(newEvents)
  await vi.waitFor(() => expect(newEvents.opened).toHaveBeenCalledTimes(1))
  oldOpen.resolve(7)
  await vi.waitFor(() => expect(mock.invoke).toHaveBeenCalledWith('native_subscription_close', { handle: 7 }))
  expect(mock.invoke.mock.calls.filter(([command]) => command === 'native_subscription_open')).toEqual([
    ['native_subscription_open', { scope: 'document', target: { kind: 'peer', peerId: 'remote' } }],
    ['native_subscription_open', { scope: 'document', target: { kind: 'peer', peerId: 'replacement' } }],
  ])
  expect(mock.invoke).not.toHaveBeenCalledWith('native_subscription_close', { handle: 8 })
  expect(oldEvents.opened).not.toHaveBeenCalled()
  expect(oldEvents.message).not.toHaveBeenCalled()
  expect(oldEvents.dropped).not.toHaveBeenCalled()
  current.close()
  read.resolve('{}')
  await Promise.resolve()
  expect(newEvents.message).not.toHaveBeenCalled()
})

it('reports an IPC open failure without attempting a WebSocket fallback', async () => {
  mock.local = false
  mock.invoke.mockRejectedValue(new Error('unauthorized'))
  const events = callbacks()
  openSubscriptionConnection(events)
  await vi.waitFor(() => expect(events.dropped).toHaveBeenCalledExactlyOnceWith(new Error('unauthorized')))
  expect(events.opened).not.toHaveBeenCalled()
  expect(mock.openSocket).not.toHaveBeenCalled()
})

it('reports a scope failure without opening a subscription or a network connection', async () => {
  mock.scope.mockRejectedValue(new Error('view_unavailable'))
  const events = callbacks()
  openSubscriptionConnection(events)
  await vi.waitFor(() => expect(events.dropped).toHaveBeenCalledExactlyOnceWith(new Error('view_unavailable')))
  expect(events.opened).not.toHaveBeenCalled()
  expect(mock.invoke).not.toHaveBeenCalled()
  expect(mock.openSocket).not.toHaveBeenCalled()
})

it('keeps an explicitly local discovery client on this computer while a remote peer is selected', async () => {
  mock.local = false
  const read = deferred<string>()
  mock.invoke.mockImplementation((command: string) => command === 'native_subscription_open'
    ? Promise.resolve(7)
    : command === 'native_subscription_read' ? read.promise : Promise.resolve())
  const client = new SubscriptionClient(openNative)
  const connected = vi.fn()
  client.onConnectionChange(connected)
  client.start()
  await vi.waitFor(() => expect(connected).toHaveBeenCalledExactlyOnceWith(true))
  expect(mock.invoke).toHaveBeenCalledWith('native_subscription_open', { scope: 'document', target: { kind: 'local' } })
  expect(mock.openSocket).not.toHaveBeenCalled()
  client.stop()
  read.resolve('{}')
  await Promise.resolve()
  expect(mock.invoke).toHaveBeenCalledWith('native_subscription_close', { handle: 7 })
})
