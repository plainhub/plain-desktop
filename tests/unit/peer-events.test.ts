import { beforeEach, expect, it, vi } from 'vitest'

const { emit, peer } = vi.hoisted(() => ({ emit: vi.fn(), peer: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit } }))
vi.mock('@/lib/device/login-peers', () => ({ findLoginPeer: peer }))


beforeEach(() => {
  vi.resetAllMocks()
  peer.mockReturnValue({ id: 'peer' })
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
