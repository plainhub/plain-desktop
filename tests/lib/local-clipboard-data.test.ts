import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('@/lib/api/peer-client', () => ({ gqlFetchPeer: vi.fn() }))
vi.mock('@/lib/device/local-mode', async (importOriginal) => {
  const actual = await importOriginal<Record<string, any>>()
  return { ...actual, isLocalMode: () => true }
})
vi.mock('@/lib/device/login-peers', async (importOriginal) => {
  const actual = await importOriginal<Record<string, any>>()
  const { ref } = await import('vue')
  return { ...actual, loginPeers: ref([]), findLoginPeer: vi.fn() }
})

import { gqlFetchPeer } from '@/lib/api/peer-client'
import { findLoginPeer } from '@/lib/device/login-peers'
import {
  peerClipboardGroups,
  clipboardDirections,
  peerClipboardDirection,
  setPeerClipboardDirection,
  gotoPeerClipboardPage,
  handlePeerClipboardEvent,
} from '@/lib/peer/local-clipboard-data'

const mockGqlFetchPeer = vi.mocked(gqlFetchPeer)
const mockFindLoginPeer = vi.mocked(findLoginPeer)

const peer = { id: 'p1', name: 'Pixel', token: 'tok' } as any

const flush = () => new Promise((r) => setTimeout(r, 0))

function addGroup(overrides: Record<string, any> = {}) {
  peerClipboardGroups.value = [{
    peerId: 'p1', name: 'Pixel', deviceType: 2, online: true, loading: false,
    loaded: false, clipboardSync: null, page: 1, total: 0, items: [],
    ...overrides,
  }] as any
}

const listReply = (reply: any) => (_peer: any, _query: string, _vars?: any) => Promise.resolve(reply)

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  peerClipboardGroups.value = []
  mockFindLoginPeer.mockReturnValue(peer)
})

describe('local clipboard data sync gating', () => {
  it('marks the group disabled when the peer answers clipboard_sync_disabled and stops asking', async () => {
    mockGqlFetchPeer.mockImplementation(listReply({ data: null, errors: [{ message: 'clipboard_sync_disabled' }] }))
    addGroup()
    await gotoPeerClipboardPage('p1', 1)
    await flush()
    const g = peerClipboardGroups.value[0]
    expect(g.clipboardSync).toBe(false)
    expect(g.online).toBe(true)
    expect(g.loaded).toBe(true)

    mockGqlFetchPeer.mockClear()
    await gotoPeerClipboardPage('p1', 1)
    await flush()
    expect(mockGqlFetchPeer).not.toHaveBeenCalled()
  })

  it('marks the group enabled on a successful list fetch', async () => {
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboard: [{ id: 'c1' }], clipboardCount: 1 } }))
    addGroup()
    await gotoPeerClipboardPage('p1', 1)
    await flush()
    const g = peerClipboardGroups.value[0]
    expect(g.clipboardSync).toBe(true)
    expect(g.items).toHaveLength(1)
    expect(g.total).toBe(1)
  })

  it('stays re-checkable (null) after a network failure and marks the group offline', async () => {
    mockGqlFetchPeer.mockRejectedValue(new Error('connection_timeout'))
    addGroup()
    await gotoPeerClipboardPage('p1', 1)
    await flush()
    expect(peerClipboardGroups.value[0].online).toBe(false)
    expect(peerClipboardGroups.value[0].clipboardSync).toBe(null)

    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboard: [], clipboardCount: 0 } }))
    await gotoPeerClipboardPage('p1', 1)
    await flush()
    expect(peerClipboardGroups.value[0].clipboardSync).toBe(true)
  })

  it('event 39 re-arms a disabled peer and reloads once the switch is on', async () => {
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboard: [{ id: 'c2' }], clipboardCount: 1 } }))
    addGroup({ clipboardSync: false })
    handlePeerClipboardEvent('p1', 39)
    await flush()
    const g = peerClipboardGroups.value[0]
    expect(g.clipboardSync).toBe(true)
    expect(g.items).toHaveLength(1)
    expect(mockGqlFetchPeer).toHaveBeenCalledOnce()
  })

  it('ignores non-clipboard events', async () => {
    addGroup({ clipboardSync: false })
    handlePeerClipboardEvent('p1', 7)
    await flush()
    expect(mockGqlFetchPeer).not.toHaveBeenCalled()
    expect(peerClipboardGroups.value[0].clipboardSync).toBe(false)
  })
})

describe('local clipboard sync direction', () => {
  beforeEach(() => {
    localStorage.removeItem('clipboard_directions')
    clipboardDirections.value = {}
  })

  it('defaults to pull so existing receive-only behavior is preserved', () => {
    expect(peerClipboardDirection('p1')).toBe('pull')
  })

  it('setting off clears the group and blocks list fetches and event 39 refreshes', async () => {
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboard: [{ id: 'c1' }], clipboardCount: 1 } }))
    addGroup({ items: [{ id: 'c1' }] as any, total: 1, clipboardSync: true, loaded: true })
    setPeerClipboardDirection('p1', 'off')
    await flush()
    const g = peerClipboardGroups.value[0]
    expect(g.items).toHaveLength(0)
    expect(g.total).toBe(0)
    expect(peerClipboardDirection('p1')).toBe('off')
    expect(JSON.parse(localStorage.getItem('clipboard_directions')!).p1).toBe('off')

    mockGqlFetchPeer.mockClear()
    await gotoPeerClipboardPage('p1', 1)
    await flush()
    handlePeerClipboardEvent('p1', 39)
    await flush()
    expect(mockGqlFetchPeer).not.toHaveBeenCalled()
  })

  it('setting both re-probes and reloads the group', async () => {
    addGroup({ clipboardSync: false })
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboard: [{ id: 'c3' }], clipboardCount: 1 } }))
    setPeerClipboardDirection('p1', 'both')
    await flush()
    const g = peerClipboardGroups.value[0]
    expect(mockGqlFetchPeer).toHaveBeenCalledOnce()
    expect(g.clipboardSync).toBe(true)
    expect(g.items).toHaveLength(1)
  })

  it('push direction disables receive without ever contacting the peer', async () => {
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboard: [{ id: 'c4' }], clipboardCount: 1 } }))
    addGroup()
    setPeerClipboardDirection('p1', 'push')
    await flush()
    expect(peerClipboardGroups.value[0].items).toHaveLength(0)
    expect(peerClipboardGroups.value[0].loaded).toBe(true)
    handlePeerClipboardEvent('p1', 39)
    await flush()
    expect(mockGqlFetchPeer).not.toHaveBeenCalled()
  })
})
