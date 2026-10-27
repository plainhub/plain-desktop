import { beforeEach, expect, it, vi } from 'vitest'
import { DeviceType } from '../../src/lib/status'

const { fetchPeer, save, copy, invoke, channels } = vi.hoisted(() => ({ fetchPeer: vi.fn(), save: vi.fn(), copy: vi.fn(), invoke: vi.fn(), channels: [] as Array<{ onmessage: (value: unknown) => void }> }))
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class { onmessage = (_value: unknown) => {}; constructor() { channels.push(this) } },
  invoke,
}))
vi.mock('@/lib/api/peer-client', () => ({ gqlFetchPeer: fetchPeer }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: vi.fn(), GqlError: class extends Error {} }))
vi.mock('@/lib/clipboard', () => ({ copyTextToClipboard: copy }))
vi.mock('@/lib/prefs/core', () => ({ get: (_key: string, fallback: unknown) => fallback, set: save }))
vi.mock('@/lib/device/login-peers', () => ({ findLoginPeer: () => ({ id: 'peer' }), loginPeers: { value: [] } }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => true }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({ pageSize: 50 }) }))

beforeEach(() => {
  vi.resetModules()
  vi.clearAllMocks()
  channels.length = 0
  invoke.mockResolvedValue(1)
  vi.stubGlobal('__IS_TAURI__', true)
})

it('refreshes displayed history while Rust exclusively owns automatic application and deduplication', async () => {
  const data = await import('../../src/lib/peer/local-clipboard-data')
  data.startLocalClipboardData()
  expect(invoke).toHaveBeenCalledWith('native_register_peer_clipboard', { channel: channels[0] })
  data.peerClipboardGroups.value.push({
    peerId: 'peer', name: 'Fixture', deviceType: DeviceType.COMPUTER,
    online: true, loading: true, loaded: false, clipboardSync: null, page: 1, total: 0, items: [],
  })
  const first = { id: 'old', text: 'Older entry', sensitive: false }
  const next = { id: 'new', text: 'Current entry', sensitive: false }
  fetchPeer.mockResolvedValueOnce({ data: { clipboardItems: [first], clipboardItemCount: 1 } })
  data.gotoPeerClipboardPage('peer', 1)
  await vi.waitFor(() => expect(data.peerClipboardGroups.value[0].items[0]?.id).toBe('old'))
  fetchPeer.mockResolvedValueOnce({ data: { clipboardItems: [next, first], clipboardItemCount: 2 } })
  channels[0].onmessage({ peerIds: ['peer'], refreshAll: false })
  await vi.waitFor(() => expect(data.peerClipboardGroups.value[0].items[0]?.id).toBe('new'))
  expect(data.peerClipboardGroups.value[0].total).toBe(2)
  expect(copy).not.toHaveBeenCalled()
  expect(save).not.toHaveBeenCalled()
})
