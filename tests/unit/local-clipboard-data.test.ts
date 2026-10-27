import { beforeEach, expect, it, vi } from 'vitest'
import { DeviceType } from '../../src/lib/status'

const { fetchPeer, save, copy, subscribe } = vi.hoisted(() => ({ fetchPeer: vi.fn(), save: vi.fn(), copy: vi.fn(), subscribe: vi.fn() }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { subscribe, onAttached: vi.fn() } }))
vi.mock('@/lib/api/peer-client', () => ({ gqlFetchPeer: fetchPeer }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: vi.fn(), GqlError: class extends Error {} }))
vi.mock('@/lib/clipboard', () => ({ copyTextToClipboard: copy }))
vi.mock('@/lib/prefs', () => ({ get: (_key: string, fallback: unknown) => fallback, set: save }))
vi.mock('@/lib/device/login-peers', () => ({ findLoginPeer: () => ({ id: 'peer' }), loginPeers: { value: [] } }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => true }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({ pageSize: 50 }) }))

beforeEach(() => {
  vi.resetModules()
  vi.clearAllMocks()
  vi.stubGlobal('__IS_TAURI__', true)
})

it('refreshes displayed history while Rust exclusively owns automatic application and deduplication', async () => {
  const data = await import('../../src/lib/peer/local-clipboard-data')
  data.startLocalClipboardData()
  expect(subscribe).toHaveBeenCalledWith('peerClipboardChanged', expect.any(Function))
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
  subscribe.mock.calls[0][1]({ peerId: 'peer' })
  await vi.waitFor(() => expect(data.peerClipboardGroups.value[0].items[0]?.id).toBe('new'))
  expect(data.peerClipboardGroups.value[0].total).toBe(2)
  expect(copy).not.toHaveBeenCalled()
  expect(save).not.toHaveBeenCalled()
})
