import { effectScope, reactive, ref } from 'vue'
import { PeerStatus } from '@/lib/status'
import { beforeEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({
  local: vi.fn(), remote: vi.fn(), save: vi.fn(), get: vi.fn(), invoke: vi.fn(),
  channels: [] as Array<{ onmessage: (value: unknown) => void }>,
}))
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class { onmessage = (_value: unknown) => {}; constructor() { mock.channels.push(this) } },
  invoke: mock.invoke,
}))
vi.mock('@/lib/api/peer-client', () => ({ gqlFetchPeer: mock.remote }))
vi.mock('@/lib/api/gql-client', () => ({ bindGraphqlClient: () => mock.local }))
vi.mock('@/lib/api/query', () => ({ clipboardItemsGQL: 'history' }))
vi.mock('@/lib/api/mutation', () => ({ deleteClipboardItemsGQL: 'delete' }))
vi.mock('@/lib/prefs/core', () => ({ get: mock.get, set: mock.save }))
vi.mock('@/lib/device/login-peers', () => ({
  getLoginPeerRevision: () => 0,
  loginPeers: { get value() { return loginPeers.value } },
}))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => true }))
vi.mock('@/stores/main', () => ({ useMainStore: () => store }))

const store = reactive({ pageSize: 25, quick: '' })
const loginPeers = ref([{ id: 'a', name: 'Phone', status: PeerStatus.PAIRED }, { id: 'b', name: 'Other device', status: PeerStatus.PAIRED }])
const entry = (id: string) => ({ data: { clipboardItems: [{ id, text: id }], clipboardItemCount: 1 } })

beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  store.quick = ''
  loginPeers.value = [{ id: 'a', name: 'Phone', status: PeerStatus.PAIRED }, { id: 'b', name: 'Other device', status: PeerStatus.PAIRED }]
  mock.channels.length = 0
  mock.invoke.mockResolvedValue(1)
  mock.get.mockReturnValue({})
  vi.stubGlobal('__IS_TAURI__', true)
})

it('queries local history first, isolates managed peers, and drops late results after switching or closing', async () => {
  const data = await import('@/lib/peer/local-clipboard-data')
  data.startLocalClipboardData()
  await vi.waitFor(() => expect(mock.channels).toHaveLength(1))
  const { useLocalClipboard } = await import('@/views/clipboard/local-clipboard')
  const scope = effectScope()
  const clipboard = scope.run(useLocalClipboard)!
  try {
    expect(mock.local).not.toHaveBeenCalled()
    mock.local.mockResolvedValue(entry('local'))
    store.quick = 'clipboard'
    await vi.waitFor(() => expect(clipboard.items.value[0]?.id).toBe('local'))
    expect(mock.remote).not.toHaveBeenCalled()
    mock.local.mockResolvedValueOnce(entry('from-desktop-host-callback'))
    mock.channels[0].onmessage({ local: true })
    await vi.waitFor(() => expect(clipboard.items.value[0]?.id).toBe('from-desktop-host-callback'))
    let stale!: (value: unknown) => void
    mock.remote.mockImplementationOnce(() => new Promise(resolve => { stale = resolve }))
    clipboard.peerId.value = 'a'
    await vi.waitFor(() => expect(mock.remote).toHaveBeenCalledWith(loginPeers.value[0], 'history', { offset: 0, limit: 25, query: '' }))
    mock.remote.mockResolvedValue(entry('peer-b'))
    clipboard.peerId.value = 'b'
    await vi.waitFor(() => expect(clipboard.items.value[0]?.id).toBe('peer-b'))
    stale(entry('old-peer-a'))
    await Promise.resolve()
    expect(clipboard.items.value[0]?.id).toBe('peer-b')
    loginPeers.value[1]!.status = PeerStatus.UNPAIRED
    await vi.waitFor(() => expect(clipboard.items.value[0]?.id).toBe('local'))
    expect(clipboard.peerId.value).toBe('')
    store.quick = ''
    await Promise.resolve()
    mock.local.mockClear()
    mock.channels[0].onmessage({ local: true })
    expect(mock.local).not.toHaveBeenCalled()
  } finally { scope.stop() }
})

it('refreshes only the selected managed peer and reports disabled and offline without leaking previous rows', async () => {
  const data = await import('@/lib/peer/local-clipboard-data')
  data.startLocalClipboardData()
  expect(mock.invoke).toHaveBeenCalledWith('native_register_peer_clipboard', { channel: mock.channels[0] })
  await Promise.resolve()
  const { useLocalClipboard } = await import('@/views/clipboard/local-clipboard')
  const scope = effectScope()
  const clipboard = scope.run(useLocalClipboard)!
  try {
    mock.remote.mockResolvedValueOnce(entry('a'))
    clipboard.peerId.value = 'a'
    store.quick = 'clipboard'
    await vi.waitFor(() => expect(clipboard.items.value[0]?.id).toBe('a'))
    mock.channels[0].onmessage({ peerIds: ['b'], refreshAll: false })
    expect(mock.remote).toHaveBeenCalledTimes(1)
    mock.remote.mockResolvedValueOnce({ errors: [{ message: 'CLIPBOARD_SYNC_DISABLED' }] })
    mock.channels[0].onmessage({ peerIds: ['a'], refreshAll: false })
    await vi.waitFor(() => expect(clipboard.status.value).toBe('disabled'))
    expect(clipboard.items.value).toEqual([])
    mock.remote.mockRejectedValueOnce(new Error('Network unavailable'))
    mock.channels[0].onmessage({ peerIds: ['a'], refreshAll: false })
    await vi.waitFor(() => expect(clipboard.status.value).toBe('offline'))
    expect(mock.save).not.toHaveBeenCalled()
  } finally { scope.stop() }
})

it('writes a canonical direction only for a managed peer and preserves the previous value on refusal', async () => {
  const data = await import('@/lib/peer/local-clipboard-data')
  expect(data.peerClipboardDirection('a')).toBe('RECEIVE')
  mock.get.mockReturnValue({ a: 'off', b: 'BOTH' })
  expect(data.peerClipboardDirection('a')).toBe('OFF')
  mock.save.mockRejectedValueOnce(new Error('Refused'))
  await expect(data.setPeerClipboardDirection('a', 'SEND')).rejects.toThrow('Refused')
  expect(data.peerClipboardDirection('a')).toBe('OFF')
  mock.save.mockImplementationOnce(async (_, values) => { mock.get.mockReturnValue(values) })
  await data.setPeerClipboardDirection('a', 'SEND')
  expect(data.peerClipboardDirection('a')).toBe('SEND')
  expect(mock.save).toHaveBeenLastCalledWith('clipboard_directions', { a: 'SEND', b: 'BOTH' })
  await expect(data.setPeerClipboardDirection('chat-only', 'SEND')).rejects.toThrow('no longer managed')
  loginPeers.value[0]!.status = PeerStatus.UNPAIRED
  await expect(data.setPeerClipboardDirection('a', 'BOTH')).rejects.toThrow('no longer managed')
  expect(mock.save).toHaveBeenCalledTimes(2)
})

it('deletes a row only from the selected device and reloads the same list', async () => {
  const { useLocalClipboard } = await import('@/views/clipboard/local-clipboard')
  const scope = effectScope()
  const clipboard = scope.run(useLocalClipboard)!
  try {
    mock.remote.mockResolvedValueOnce(entry('peer-only'))
      .mockResolvedValueOnce({ data: { deleteClipboardItems: { affectedCount: 1 } } })
      .mockResolvedValueOnce({ data: { clipboardItems: [], clipboardItemCount: 0 } })
    clipboard.peerId.value = 'a'
    store.quick = 'clipboard'
    await vi.waitFor(() => expect(clipboard.items.value[0]?.id).toBe('peer-only'))
    await clipboard.deleteItem(clipboard.items.value[0]!)
    expect(mock.remote.mock.calls).toEqual([
      [loginPeers.value[0], 'history', { offset: 0, limit: 25, query: '' }],
      [loginPeers.value[0], 'delete', { query: 'ids:peer-only' }],
      [loginPeers.value[0], 'history', { offset: 0, limit: 25, query: '' }],
    ])
    expect(mock.local).not.toHaveBeenCalled()
    expect(clipboard.items.value).toEqual([])
  } finally { scope.stop() }
})
