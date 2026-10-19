import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { DeviceType } from '../../src/lib/status'

const { native, remote, listen } = vi.hoisted(() => ({ native: vi.fn(), remote: vi.fn(), listen: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ listen }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: native }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: remote }))

import { loginPeers, preloadLoginPeers, clearLoginPeer, subscribeLoginPeerUpdates } from '../../src/lib/device/login-peers'

beforeEach(() => {
  vi.resetAllMocks()
  vi.stubGlobal('__IS_TAURI__', true)
  loginPeers.value = []
  remote.mockRejectedValue(new Error('selected remote core must not receive local session writes'))
})
afterEach(() => vi.unstubAllGlobals())

it('projects management grants from the canonical local peers query', async () => {
  const peer = {
    id: 'peer',
    name: 'Device',
    token: 'token',
    publicKey: 'signature',
    port: 9443,
    ip: '192.0.2.1',
    status: 'PAIRED',
    deviceType: DeviceType.COMPUTER,
    createdAt: '',
    updatedAt: '',
    online: false,
  }
  native.mockResolvedValueOnce({ data: { peers: [peer, { ...peer, id: 'chat-only', token: '' }] } })
  await preloadLoginPeers()
  expect(native).toHaveBeenCalledTimes(1)
  expect(native.mock.calls[0][0].query).toContain('peers {')
  expect(loginPeers.value).toEqual([peer])
  expect(remote).not.toHaveBeenCalled()
})

it('clears sessions through the local main schema independently of the selected remote target', async () => {
  loginPeers.value = [{ id: 'peer', name: 'Old', token: 'token', publicKey: 'signature' } as (typeof loginPeers.value)[number]]
  native.mockResolvedValueOnce({ data: { logoutPeer: true } }).mockResolvedValueOnce({ data: { peers: [] } })
  await clearLoginPeer('peer')
  expect(native.mock.calls[0][0].query).toContain('logoutPeer')
  expect(loginPeers.value).toEqual([])
  expect(remote).not.toHaveBeenCalled()
})

it('subscribes to actual local peer changes and reconciles immediately after registration', async () => {
  const stop = vi.fn()
  let changed: (() => Promise<void>) | undefined
  listen.mockImplementation(async (type, handler) => {
    expect(type).toBe('PEER_UPDATED')
    changed = handler
    return stop
  })
  const row = { id: 'peer', name: 'New', token: 'token', createdAt: '' }
  native.mockResolvedValue({ data: { peers: [row] } })
  const unlisten = await subscribeLoginPeerUpdates()
  expect(loginPeers.value[0].name).toBe('New')
  native.mockResolvedValue({ data: { peers: [{ ...row, name: 'Renamed' }] } })
  await changed!()
  expect(loginPeers.value[0].name).toBe('Renamed')
  expect(native).toHaveBeenCalledTimes(2)
  expect(remote).not.toHaveBeenCalled()
  unlisten()
  expect(stop).toHaveBeenCalledTimes(1)
})

it('coalesces changes while a snapshot is pending and never applies an older result after a newer one', async () => {
  let resolve: (value: unknown) => void = () => {}
  native
    .mockReturnValueOnce(
      new Promise((done) => {
        resolve = done
      })
    )
    .mockResolvedValueOnce({ data: { peers: [{ id: 'new', token: 'token', createdAt: '' }] } })
  const first = preloadLoginPeers()
  const pending = Array.from({ length: 10 }, () => preloadLoginPeers())
  expect(native).toHaveBeenCalledTimes(1)
  resolve({ data: { peers: [{ id: 'old', token: 'token', createdAt: '' }] } })
  await Promise.all([first, ...pending])
  expect(native).toHaveBeenCalledTimes(2)
  expect(loginPeers.value.map((peer) => peer.id)).toEqual(['new'])
})
