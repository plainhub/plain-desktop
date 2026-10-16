import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { DeviceType } from '../../src/lib/status'

const { native, remote } = vi.hoisted(() => ({ native: vi.fn(), remote: vi.fn() }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: native }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: remote }))

import { loginPeers, saveLoginPeer, clearLoginPeer, updateLoginPeerName } from '../../src/lib/device/login-peers'

beforeEach(() => {
  vi.resetAllMocks()
  vi.stubGlobal('__IS_TAURI__', true)
  loginPeers.value = []
  remote.mockRejectedValue(new Error('selected remote core must not receive local session writes'))
})
afterEach(() => vi.unstubAllGlobals())

it('saves local login state through the main native schema and projects the returned snapshot', async () => {
  const peer = { id: 'peer', name: 'Device', token: 'token', publicKey: 'signature', port: 9443, ip: '192.0.2.1', status: 'UNPAIRED', deviceType: DeviceType.COMPUTER, createdAt: '', updatedAt: '' }
  native.mockResolvedValueOnce({ data: { loginPeer: true } })
    .mockResolvedValueOnce({ data: { loggedInPeers: [{ peer, online: false }] } })
  await saveLoginPeer({ clientId: 'peer', name: 'Device', host: '192.0.2.1:9443', deviceType: DeviceType.COMPUTER, token: 'token', signaturePublicKey: 'signature' })
  expect(native).toHaveBeenCalledTimes(2)
  expect(native.mock.calls[0][0].query).toContain('loginPeer(input: $input)')
  expect(native.mock.calls[0][0].variables.input).toMatchObject({ id: 'peer', host: '192.0.2.1:9443', token: 'token' })
  expect(loginPeers.value).toEqual([{ ...peer, online: false }])
  expect(remote).not.toHaveBeenCalled()
})

it('propagates a GraphQL write refusal before refreshing or mirroring a token', async () => {
  native.mockResolvedValue({ errors: [{ message: 'no_permission' }] })
  await expect(saveLoginPeer({ clientId: 'peer', name: 'Device', host: '192.0.2.1:9443', deviceType: DeviceType.COMPUTER, token: 'token', signaturePublicKey: 'signature' })).rejects.toThrow('no_permission')
  expect(native).toHaveBeenCalledTimes(1)
  expect(loginPeers.value).toEqual([])
  expect(remote).not.toHaveBeenCalled()
})

it('renames and clears sessions through the local main schema independently of the selected remote target', async () => {
  loginPeers.value = [{ id: 'peer', name: 'Old', token: 'token', publicKey: 'signature' } as typeof loginPeers.value[number]]
  native.mockResolvedValueOnce({ data: { updatePeerName: true } })
    .mockResolvedValueOnce({ data: { logoutPeer: true } })
    .mockResolvedValueOnce({ data: { loggedInPeers: [] } })
  await updateLoginPeerName('peer', 'New')
  expect(loginPeers.value[0].name).toBe('New')
  await clearLoginPeer('peer')
  expect(native.mock.calls[0][0].query).toContain('updatePeerName')
  expect(native.mock.calls[1][0].query).toContain('logoutPeer')
  expect(loginPeers.value).toEqual([])
  expect(remote).not.toHaveBeenCalled()
})
