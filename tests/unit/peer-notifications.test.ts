import { beforeEach, expect, it, vi } from 'vitest'

const { handlers, sound, notify, request } = vi.hoisted(() => ({
  handlers: new Map<string, (value: any) => void>(),
  sound: vi.fn(),
  notify: vi.fn(),
  request: vi.fn(),
}))
vi.mock('@/plugins/eventbus', () => ({ default: { on: (type: string, handler: (value: any) => void) => handlers.set(type, handler), emit: vi.fn() } }))
vi.mock('@/lib/api/api', () => ({ proxyHttpUrl: (url: string) => `proxy:${url}` }))
vi.mock('@/lib/api/peer-client', () => ({ gqlFetchPeer: request }))
vi.mock('@/lib/api/mutation', () => ({ deleteNotificationsGQL: 'delete-fixture', replyNotificationGQL: 'reply-fixture' }))
vi.mock('@/lib/device/login-peers', () => ({ findLoginPeer: () => ({ id: 'peer' }) }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => true }))
vi.mock('@/lib/notification-sound', () => ({ playNotificationSound: sound }))
vi.mock('@/lib/desktop-notification', () => ({ showDesktopNotification: notify }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({ notificationVolume: { value: 1 } }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))

beforeEach(() => {
  vi.resetModules()
  vi.clearAllMocks()
  handlers.clear()
  vi.stubGlobal('__IS_TAURI__', true)
})

function item(id: string) {
  return {
    id,
    onlyOnce: false,
    isClearable: true,
    appId: 'fixture.app',
    appName: 'Fixture',
    postedAt: '',
    silent: false,
    title: id,
    body: '',
    icon: 'https://device.example/fs?id=fixture',
    actions: [],
    replyActions: [],
  }
}

function group(peerId: string) {
  return {
    peerId,
    name: peerId,
    deviceType: 'COMPUTER',
    online: true,
    loading: false,
    loaded: true,
    capabilities: ['NOTIFICATIONS'],
    permissions: ['NOTIFICATION_LISTENER'],
    items: [item('old')],
  } as any
}

it('projects recovery and peer deltas, rejects stale frames and removes retired peers', async () => {
  const { applyPeerNotificationUpdate: apply, peerNotificationGroups: groups } = await import('../../src/lib/peer/local-peer-data')
  expect(apply({ revision: 10, groups: [group('peer'), group('other')], alerts: [item('never')] })).toEqual([])
  expect(groups.value[0].items[0].icon).toBe('proxy:https://device.example/fs?id=fixture')
  const changed = group('peer')
  changed.items = [item('new')]
  expect(apply({ revision: 11, peerId: 'peer', group: changed, alerts: [item('new')] })).toHaveLength(1)
  expect(apply({ revision: 11, peerId: 'peer', group: group('peer'), alerts: [item('new')] })).toEqual([])
  expect(apply({ revision: 9, groups: [] })).toEqual([])
  expect(groups.value.map((g) => g.items[0].id)).toEqual(['new', 'old'])
  apply({ revision: 12, peerId: 'peer', group: null })
  expect(groups.value.map((g) => g.peerId)).toEqual(['other'])
  apply({ revision: 13, groups: [] })
  expect(groups.value).toEqual([])
})

it('uses capabilities for visibility and only projects the online status supplied by Rust', async () => {
  const { applyPeerNotificationUpdate: apply, peerNotificationGroups: groups } = await import('../../src/lib/peer/local-peer-data')
  const unsupported = group('unsupported')
  unsupported.capabilities = []
  const offline = group('supported')
  offline.online = false
  const unknown = group('unknown')
  unknown.capabilities = null
  unknown.online = false
  apply({ revision: 1, groups: [unsupported, offline, unknown] })
  expect(groups.value.map((g) => g.peerId)).toEqual(['supported', 'unknown'])
  expect(groups.value[0].online).toBe(false)
  expect(groups.value[0].items).toHaveLength(1)
})

it('presents shared alerts once without scheduling client refreshes or fetching snapshots', async () => {
  vi.useFakeTimers()
  try {
    const { startLocalPeerData } = await import('../../src/lib/peer/local-peer-data')
    startLocalPeerData()
    startLocalPeerData()
    const update = handlers.get('peer_notifications_updated')!
    expect(update).toBeTypeOf('function')
    update({ revision: 1, groups: [group('peer')] })
    const created = { revision: 2, peerId: 'peer', group: group('peer'), alerts: [item('new')] }
    update(created)
    update(created)
    expect(sound).toHaveBeenCalledTimes(1)
    expect(notify).toHaveBeenCalledTimes(1)
    expect(request).not.toHaveBeenCalled()
    expect(vi.getTimerCount()).toBe(0)
  } finally {
    vi.useRealTimers()
  }
})
