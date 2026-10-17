import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { DeviceType } from '../../src/lib/status'

const { native, remote } = vi.hoisted(() => ({ native: vi.fn(), remote: vi.fn() }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: native }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: remote }))

import { loginPeers, preloadLoginPeers, clearLoginPeer, updateLoginPeerName } from '../../src/lib/device/login-peers'

beforeEach(() => {
  vi.resetAllMocks()
  vi.stubGlobal('__IS_TAURI__', true)
  loginPeers.value = []
  remote.mockRejectedValue(new Error('selected remote core must not receive local session writes'))
})
afterEach(() => vi.unstubAllGlobals())

it('projects management grants from the canonical local peers query', async () => {
  const peer = { id: 'peer', name: 'Device', token: 'token', publicKey: 'signature', port: 9443, ip: '192.0.2.1', status: 'PAIRED', deviceType: DeviceType.COMPUTER, createdAt: '', updatedAt: '', online: false }
  native.mockResolvedValueOnce({ data: { peers: [peer, { ...peer, id: 'chat-only', token: '' }] } })
  await preloadLoginPeers()
  expect(native).toHaveBeenCalledTimes(1)
  expect(native.mock.calls[0][0].query).toContain('peers {')
  expect(loginPeers.value).toEqual([peer])
  expect(remote).not.toHaveBeenCalled()
})

it('renames and clears sessions through the local main schema independently of the selected remote target', async () => {
  loginPeers.value = [{ id: 'peer', name: 'Old', token: 'token', publicKey: 'signature' } as typeof loginPeers.value[number]]
  native.mockResolvedValueOnce({ data: { updatePeerName: true } })
    .mockResolvedValueOnce({ data: { logoutPeer: true } })
    .mockResolvedValueOnce({ data: { peers: [] } })
  await updateLoginPeerName('peer', 'New')
  expect(loginPeers.value[0].name).toBe('New')
  await clearLoginPeer('peer')
  expect(native.mock.calls[0][0].query).toContain('updatePeerName')
  expect(native.mock.calls[1][0].query).toContain('logoutPeer')
  expect(loginPeers.value).toEqual([])
  expect(remote).not.toHaveBeenCalled()
})
