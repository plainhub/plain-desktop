import { beforeEach, afterEach, expect, it, vi } from 'vitest'

const fixture = vi.hoisted(() => ({
  bind: vi.fn(), request: vi.fn(), scanning: false, listeners: new Map<string, (...args: any[]) => void>(), subscriptions: new Map<string, (...args: any[]) => void>(),
}))
vi.mock('@/lib/api/gql-client', () => ({ bindGraphqlClient: fixture.bind }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe: (name: string, handler: (...args: any[]) => void) => { fixture.subscriptions.set(name, handler); return () => {} },
  onAttached: (handler: (...args: any[]) => void) => { fixture.subscriptions.set('attached', handler); return () => {} },
} }))
vi.mock('@/lib/api/mutation', () => ({ startDiscoveryGQL: 'startDiscovery', stopDiscoveryGQL: 'stopDiscovery' }))
vi.mock('@/lib/api/query', () => ({ nearbyDevicesGQL: 'nearbyDevices', isDiscoveringGQL: 'isDiscovering' }))
vi.mock('@/plugins/eventbus', () => ({ default: { on: (name: string, handler: (...args: any[]) => void) => fixture.listeners.set(name, handler) } }))
vi.mock('@/stores/chat', () => ({ useChatStore: () => ({ fetchPeers: vi.fn(), findPeer: () => null }) }))
vi.mock('@/lib/status', () => ({ PeerStatus: { PAIRED: 'PAIRED', UNPAIRED: 'UNPAIRED' } }))

beforeEach(() => {
  vi.useFakeTimers()
  vi.resetModules()
  vi.resetAllMocks()
  fixture.listeners.clear()
  fixture.subscriptions.clear()
  fixture.scanning = false
  fixture.request.mockImplementation(async (document: string) => {
    if (document === 'startDiscovery') fixture.scanning = true
    if (document === 'stopDiscovery') fixture.scanning = false
    return { data: document === 'nearbyDevices' ? { nearbyDevices: [], mdnsState: { scanning: fixture.scanning } } : { isDiscovering: fixture.scanning } }
  })
  fixture.bind.mockReturnValue(fixture.request)
})
afterEach(() => vi.useRealTimers())

const calls = (document: string) => fixture.request.mock.calls.filter(([query]) => query === document).length

it('checks every three seconds and leaves an existing scan running', async () => {
  fixture.scanning = true
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const view = useDeviceDiscovery()
  await view.start()
  await view.start()
  expect(calls('isDiscovering')).toBe(1)
  expect(calls('startDiscovery')).toBe(0)
  await vi.advanceTimersByTimeAsync(2999)
  expect(calls('isDiscovering')).toBe(1)
  await vi.advanceTimersByTimeAsync(1)
  expect(calls('isDiscovering')).toBe(2)
  await view.stop()
  expect(calls('stopDiscovery')).toBe(1)
  expect(vi.getTimerCount()).toBe(0)
})

it('restarts a stopped scan on the next poll and never polls a closed page', async () => {
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const view = useDeviceDiscovery()
  await view.start()
  expect(calls('startDiscovery')).toBe(1)
  fixture.scanning = false
  await vi.advanceTimersByTimeAsync(3000)
  expect(calls('startDiscovery')).toBe(2)
  await vi.advanceTimersByTimeAsync(3000)
  expect(calls('startDiscovery')).toBe(2)
  await view.stop()
  const checks = calls('isDiscovering')
  await vi.advanceTimersByTimeAsync(9000)
  expect(calls('isDiscovering')).toBe(checks)
})

it('stops globally when either page closes and lets the other page restart on its own poll', async () => {
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const first = useDeviceDiscovery()
  const second = useDeviceDiscovery()
  await first.start()
  await second.start()
  expect(calls('startDiscovery')).toBe(1)
  await first.stop()
  expect(calls('stopDiscovery')).toBe(1)
  expect(fixture.scanning).toBe(false)
  await vi.advanceTimersByTimeAsync(3000)
  expect(calls('startDiscovery')).toBe(2)
  await second.stop()
  expect(calls('stopDiscovery')).toBe(2)
})

it('does not restart after a pending status query returns to a closed page', async () => {
  let finish!: (result: { data: { isDiscovering: boolean } }) => void
  let entered!: () => void
  const requested = new Promise<void>(resolve => { entered = resolve })
  fixture.request.mockImplementationOnce(() => { entered(); return new Promise(resolve => { finish = resolve }) })
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const view = useDeviceDiscovery()
  const opening = view.start()
  await requested
  const closing = view.stop()
  finish({ data: { isDiscovering: false } })
  await Promise.all([opening, closing])
  expect(calls('startDiscovery')).toBe(0)
  expect(calls('stopDiscovery')).toBe(1)
  expect(vi.getTimerCount()).toBe(0)
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

it('uses device notifications for list refresh without starting the scanner', async () => {
  fixture.scanning = true
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const view = useDeviceDiscovery()
  await view.start()
  fixture.scanning = false
  await fixture.subscriptions.get('nearbyDevicesChanged')!(true)
  expect(calls('startDiscovery')).toBe(0)
  expect(fixture.subscriptions.has('mdnsChanged')).toBe(false)
  await vi.advanceTimersByTimeAsync(3000)
  expect(calls('startDiscovery')).toBe(1)
  await view.stop()
})
