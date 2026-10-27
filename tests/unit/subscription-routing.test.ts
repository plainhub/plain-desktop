import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { openSubscriptionConnection } from '@/lib/api/subscription-connection'

const mock = vi.hoisted(() => ({ local: true, invoke: vi.fn(), socket: { close: vi.fn() }, openSocket: vi.fn() }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => mock.local }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: mock.invoke }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: () => 'wss://remote.example:8443' }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => 'session' }))
vi.mock('@/lib/device/client-id', () => ({ getDesktopClientId: () => 'self', getRemoteClientId: () => 'remote' }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: vi.fn() }))
vi.mock('@/lib/api/time-sync', () => ({ getSyncedTimestamp: () => 0 }))
vi.mock('@/lib/api/http', () => ({ openSocket: mock.openSocket }))
beforeEach(() => {
  vi.resetAllMocks()
  vi.useFakeTimers()
  vi.stubGlobal('__IS_TAURI__', true)
})
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals() })

it('opens Desktop remote subscriptions over the network and clears its handshake timer on close', () => {
  mock.local = false
  mock.openSocket.mockReturnValue(mock.socket)
  const callbacks = { opened: vi.fn(), message: vi.fn(), dropped: vi.fn() }
  const connection = openSubscriptionConnection(callbacks)
  expect(mock.openSocket).toHaveBeenCalledWith('wss://remote.example:8443/graphql?cid=self', 'remote', 'plain-graphql-ws-v1')
  expect(mock.invoke).not.toHaveBeenCalled()
  connection.close()
  expect(mock.socket.close).toHaveBeenCalledTimes(1)
  expect(vi.getTimerCount()).toBe(0)
})

it('opens local subscriptions through native IPC and closes without delivering late data', async () => {
  mock.local = true
  let finishRead!: (value: string) => void
  mock.invoke.mockImplementation((command: string) => command === 'native_subscription_open'
    ? Promise.resolve(7)
    : command === 'native_subscription_read'
      ? new Promise(resolve => { finishRead = resolve })
      : Promise.resolve())
  const callbacks = { opened: vi.fn(), message: vi.fn(), dropped: vi.fn() }
  const connection = openSubscriptionConnection(callbacks)
  await Promise.resolve()
  expect(callbacks.opened).toHaveBeenCalledTimes(1)
  expect(mock.openSocket).not.toHaveBeenCalled()
  connection.close()
  finishRead('{}')
  await Promise.resolve()
  expect(mock.invoke).toHaveBeenCalledWith('native_subscription_close', { handle: 7 })
  expect(callbacks.message).not.toHaveBeenCalled()
})
