import { beforeEach, expect, it, vi } from 'vitest'

const { emit, peer } = vi.hoisted(() => ({ emit: vi.fn(), peer: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit } }))
vi.mock('@/lib/device/login-peers', () => ({ findLoginPeer: peer }))

import { projectPeerManagementEvent } from '../../src/lib/peer/peer-events'

beforeEach(() => {
  vi.resetAllMocks()
  peer.mockReturnValue({ id: 'peer' })
})

it('projects native management events without socket or cryptographic logic in the view', () => {
  const data = { id: 'notification', body: 'Fixture' }
  projectPeerManagementEvent({ peerId: 'peer', type: 'NOTIFICATION_CREATED', data }, true)
  expect(emit).toHaveBeenCalledWith('peer_ws_event', { peerId: 'peer', type: 'NOTIFICATION_CREATED', data })
})

it('ignores remote events even if they impersonate a known peer and claim a native origin', () => {
  projectPeerManagementEvent({ peerId: 'peer', type: 'CLIPBOARD_CHANGED', data: { text: 'fixture' }, native: true }, false)
  expect(emit).not.toHaveBeenCalled()
  expect(peer).not.toHaveBeenCalled()
})

it('ignores events for removed devices and malformed payloads', () => {
  for (const event of [null, 'text', {}, { peerId: 1, type: 'CLIPBOARD_CHANGED', data: {} }, { peerId: 'peer', type: 1, data: {} }, { peerId: 'peer', type: 'CLIPBOARD_CHANGED', data: null }]) {
    projectPeerManagementEvent(event, true)
  }
  peer.mockReturnValue(undefined)
  projectPeerManagementEvent({ peerId: 'removed', type: 'CLIPBOARD_CHANGED', data: { text: 'fixture' } }, true)
  expect(emit).not.toHaveBeenCalled()
})

it('only projects aggregated notification snapshots from the trusted native stream', async () => {
  const { projectPeerNotificationEvent } = await import('../../src/lib/peer/peer-events')
  const snapshot = { revision: 1, groups: [] }
  projectPeerNotificationEvent(snapshot, false)
  expect(emit).not.toHaveBeenCalled()
  for (const value of [null, {}, { revision: -1 }, { revision: Number.NaN }]) projectPeerNotificationEvent(value, true)
  expect(emit).not.toHaveBeenCalled()
  projectPeerNotificationEvent(snapshot, true)
  expect(emit).toHaveBeenCalledWith('peer_notifications_updated', snapshot)
  expect(peer).not.toHaveBeenCalled()
})
