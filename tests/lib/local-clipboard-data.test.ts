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
  appliedClipboardIds,
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
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboardItems: [{ id: 'c1' }], clipboardItemCount: 1 } }))
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

    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboardItems: [], clipboardItemCount: 0 } }))
    await gotoPeerClipboardPage('p1', 1)
    await flush()
    expect(peerClipboardGroups.value[0].clipboardSync).toBe(true)
  })

  it('event 39 re-arms a disabled peer and reloads once the switch is on', async () => {
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboardItems: [{ id: 'c2' }], clipboardItemCount: 1 } }))
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
  const writeText = vi.fn(() => Promise.resolve())

  beforeEach(() => {
    localStorage.removeItem('clipboard_directions')
    localStorage.removeItem('clipboard_applied')
    clipboardDirections.value = {}
    appliedClipboardIds.value = {}
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
    writeText.mockClear()
  })

  it('defaults to pull so existing receive-only behavior is preserved', () => {
    expect(peerClipboardDirection('p1')).toBe('pull')
  })

  it('direction never affects the history list — off still fetches and shows entries', async () => {
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboardItems: [{ id: 'c1', text: 'x' }], clipboardItemCount: 1 } }))
    addGroup()
    setPeerClipboardDirection('p1', 'off')
    await flush()
    expect(mockGqlFetchPeer).toHaveBeenCalledOnce()
    expect(peerClipboardGroups.value[0].items).toHaveLength(1)
    expect(writeText).not.toHaveBeenCalled()
  })

  it('first sight only seeds the baseline without touching the system clipboard', async () => {
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboardItems: [{ id: 'c1', text: 'old' }], clipboardItemCount: 1 } }))
    addGroup()
    await gotoPeerClipboardPage('p1', 1)
    await flush()
    expect(writeText).not.toHaveBeenCalled()
  })

  it('applies a newer entry into the system clipboard on event 39 when receiving', async () => {
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboardItems: [{ id: 'c1', text: 'old' }], clipboardItemCount: 1 } }))
    addGroup()
    await gotoPeerClipboardPage('p1', 1)
    await flush()
    expect(writeText).not.toHaveBeenCalled()

    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboardItems: [{ id: 'c2', text: 'fresh' }, { id: 'c1', text: 'old' }], clipboardItemCount: 2 } }))
    handlePeerClipboardEvent('p1', 39)
    await flush()
    expect(writeText).toHaveBeenCalledExactlyOnceWith('fresh')
  })

  it('never auto-applies sensitive entries', async () => {
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboardItems: [{ id: 'c1', text: 'old' }], clipboardItemCount: 1 } }))
    addGroup()
    await gotoPeerClipboardPage('p1', 1)
    await flush()
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboardItems: [{ id: 'c2', text: 'secret', sensitive: true }], clipboardItemCount: 2 } }))
    handlePeerClipboardEvent('p1', 39)
    await flush()
    expect(writeText).not.toHaveBeenCalled()
  })

  it('push direction still loads history but never writes the system clipboard', async () => {
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboardItems: [{ id: 'c9', text: 'new' }], clipboardItemCount: 1 } }))
    addGroup()
    setPeerClipboardDirection('p1', 'push')
    await flush()
    handlePeerClipboardEvent('p1', 39)
    await flush()
    expect(peerClipboardGroups.value[0].items).toHaveLength(1)
    expect(writeText).not.toHaveBeenCalled()
  })

  it('changing direction re-probes a phone-disabled group', async () => {
    addGroup({ clipboardSync: false })
    mockGqlFetchPeer.mockImplementation(listReply({ data: { clipboardItems: [{ id: 'c5', text: 'x' }], clipboardItemCount: 1 } }))
    setPeerClipboardDirection('p1', 'both')
    await flush()
    expect(mockGqlFetchPeer).toHaveBeenCalledOnce()
    expect(peerClipboardGroups.value[0].clipboardSync).toBe(true)
  })
})
