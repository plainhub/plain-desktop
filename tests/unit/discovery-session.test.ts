import { beforeEach, afterEach, expect, it, vi } from 'vitest'

const fixture = vi.hoisted(() => ({
  bind: vi.fn(), request: vi.fn(), listeners: new Map<string, (...args: any[]) => void>(), subscriptions: new Map<string, (...args: any[]) => void>(),
}))
vi.mock('@/lib/api/gql-client', () => ({ bindGraphqlClient: fixture.bind }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe: (name: string, handler: (...args: any[]) => void) => { fixture.subscriptions.set(name, handler); return () => {} },
  onAttached: (handler: (...args: any[]) => void) => { fixture.subscriptions.set('attached', handler); return () => {} },
} }))
vi.mock('@/lib/api/mutation', () => ({ startDiscoveryGQL: 'startDiscovery', stopDiscoveryGQL: 'stopDiscovery' }))
vi.mock('@/lib/api/query', () => ({ nearbyDevicesGQL: 'nearbyDevices' }))
vi.mock('@/plugins/eventbus', () => ({ default: { on: (name: string, handler: (...args: any[]) => void) => fixture.listeners.set(name, handler) } }))
vi.mock('@/stores/chat', () => ({ useChatStore: () => ({ fetchPeers: vi.fn(), findPeer: () => null }) }))
vi.mock('@/lib/status', () => ({ PeerStatus: { PAIRED: 'PAIRED', UNPAIRED: 'UNPAIRED' } }))

beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  fixture.listeners.clear()
  fixture.subscriptions.clear()
  fixture.request.mockImplementation(async (document: string) => ({ data: document === 'nearbyDevices' ? { nearbyDevices: [], mdnsState: { scanning: true } } : {} }))
  fixture.bind.mockReturnValue(fixture.request)
})
afterEach(() => vi.useRealTimers())

it('starts and stops the global scanner without tokens or polling', async () => {
  vi.useFakeTimers()
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const first = useDeviceDiscovery()
  const second = useDeviceDiscovery()
  await first.start()
  await first.start()
  await second.start()
  expect(fixture.request.mock.calls.filter(([document]) => document === 'startDiscovery')).toEqual([['startDiscovery'], ['startDiscovery']])
  await first.stop()
  await first.stop()
  expect(fixture.request.mock.calls.filter(([document]) => document === 'stopDiscovery')).toEqual([['stopDiscovery']])
  await second.stop()
  expect(fixture.request.mock.calls.filter(([document]) => document === 'stopDiscovery')).toHaveLength(2)
  expect(vi.getTimerCount()).toBe(0)
})

it('balances an in-flight old view against its original transport before starting another', async () => {
  let finish!: () => void
  const blocked = new Promise<void>((resolve) => { finish = resolve })
  const oldClient = vi.fn(async (document: string) => {
    if (document === 'startDiscovery') await blocked
    return { data: {} }
  })
  const newClient = vi.fn(async (document: string) => ({ data: document === 'nearbyDevices' ? { nearbyDevices: [], mdnsState: { scanning: true } } : {} }))
  fixture.bind.mockReturnValueOnce(oldClient).mockReturnValueOnce(newClient)
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const view = useDeviceDiscovery()
  const opening = view.start()
  await vi.waitFor(() => expect(oldClient).toHaveBeenCalledTimes(1))
  const closing = view.stop()
  const reopened = view.start()
  finish()
  await Promise.all([opening, closing, reopened])
  expect(oldClient.mock.calls.map(([document]) => document)).toEqual(['startDiscovery', 'stopDiscovery'])
  expect(newClient.mock.calls.map(([document]) => document)).toEqual(['startDiscovery', 'nearbyDevices'])
  await view.stop()
})

it('reconnect restores only active views without an interval that resurrects closed views', async () => {
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const first = useDeviceDiscovery()
  const second = useDeviceDiscovery()
  await first.start()
  await second.start()
  await first.stop()
  fixture.subscriptions.get('attached')!()
  await vi.waitFor(() => expect(fixture.request.mock.calls.filter(([document]) => document === 'startDiscovery')).toHaveLength(3))
  expect(fixture.request.mock.calls.filter(([document]) => document === 'startDiscovery').at(-1)).toEqual(['startDiscovery'])
  await second.stop()
})

it('restores pending pairing from the same discovery snapshot and projects terminal states', async () => {
  const device = { id: 'peer', name: 'Fixture', status: 'UNPAIRED' }
  fixture.request.mockImplementation(async (document: string) => ({ data: document === 'nearbyDevices' ? { nearbyDevices: [device], mdnsState: { scanning: true }, pairingStates: [{ deviceId: 'peer', generation: 'current', mode: 'CHAT', phase: 'PAIRING', error: null }] } : {} }))
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const view = useDeviceDiscovery()
  await view.start()
  expect(view.devices.value[0].status).toBe('PAIRING')
  fixture.listeners.get('PAIRING_FAILED')!({ deviceId: 'peer' })
  expect(view.devices.value[0].status).toBe('PAIRING')
  fixture.request.mockImplementation(async (document: string) => ({ data: document === 'nearbyDevices' ? { nearbyDevices: [device], mdnsState: { scanning: true }, pairingStates: [{ deviceId: 'peer', generation: 'current', mode: 'CHAT', phase: 'FAILED', error: 'failed' }] } : {} }))
  fixture.subscriptions.get('pairingChanged')!(true)
  await vi.waitFor(() => expect(view.devices.value[0].status).toBe('UNPAIRED'))
  await view.stop()
})

it('refreshes the discovery snapshot for subscription events without reacquiring', async () => {
  const { useDeviceDiscovery, DiscoveryStatus } = await import('../../src/hooks/use-device-discovery')
  const view = useDeviceDiscovery()
  await view.start()
  fixture.request.mockImplementation(async (document: string) => ({ data: document === 'nearbyDevices'
    ? { nearbyDevices: [{ id: 'found', status: 'UNPAIRED' }], mdnsState: { scanning: true } }
    : {} }))
  fixture.subscriptions.get('nearbyDevicesChanged')!(true)
  await vi.waitFor(() => expect(view.devices.value[0]?.id).toBe('found'))
  expect(view.status.value).toBe(DiscoveryStatus.OK)
  expect(fixture.request.mock.calls.filter(([document]) => document === 'startDiscovery')).toHaveLength(1)
  await view.stop()
})

it('coalesces discovery changes while a snapshot is in flight', async () => {
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const view = useDeviceDiscovery()
  await view.start()
  let finish!: () => void
  const blocked = new Promise<void>(resolve => { finish = resolve })
  fixture.request.mockImplementation(async (document: string) => {
    if (document === 'nearbyDevices') await blocked
    return { data: document === 'nearbyDevices' ? { nearbyDevices: [], mdnsState: { scanning: true } } : {} }
  })
  const changed = fixture.subscriptions.get('nearbyDevicesChanged')!
  changed(true)
  await vi.waitFor(() => expect(fixture.request.mock.calls.filter(([document]) => document === 'nearbyDevices')).toHaveLength(2))
  changed(true)
  changed(true)
  finish()
  await vi.waitFor(() => expect(fixture.request.mock.calls.filter(([document]) => document === 'nearbyDevices')).toHaveLength(3))
  await view.stop()
})

it('restarts a stopped scan only while the page is active', async () => {
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const view = useDeviceDiscovery()
  await view.start()
  fixture.request.mockImplementation(async (document: string) => ({ data: document === 'nearbyDevices' ? { nearbyDevices: [], mdnsState: { scanning: false } } : {} }))
  const changed = fixture.subscriptions.get('mdnsChanged')!
  await changed(true)
  expect(fixture.request.mock.calls.filter(([document]) => document === 'startDiscovery')).toHaveLength(2)
  await view.stop()
  await changed(true)
  expect(fixture.request.mock.calls.filter(([document]) => document === 'startDiscovery')).toHaveLength(2)
})
