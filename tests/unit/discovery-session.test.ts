import { beforeEach, afterEach, expect, it, vi } from 'vitest'

const fixture = vi.hoisted(() => ({
  bind: vi.fn(), request: vi.fn(), listeners: new Map<string, (...args: any[]) => void>(), nextId: 0,
}))
vi.mock('@/lib/api/gql-client', () => ({ bindGraphqlClient: fixture.bind }))
vi.mock('@/lib/api/mutation', () => ({ acquireDiscoveryGQL: 'acquireDiscovery', releaseDiscoveryGQL: 'releaseDiscovery' }))
vi.mock('@/lib/api/query', () => ({ nearbyDevicesGQL: 'nearbyDevices' }))
vi.mock('@/plugins/eventbus', () => ({ default: { on: (name: string, handler: (...args: any[]) => void) => fixture.listeners.set(name, handler) } }))
vi.mock('@/stores/chat', () => ({ useChatStore: () => ({ fetchPeers: vi.fn() }) }))
vi.mock('@/lib/status', () => ({ PeerStatus: { PAIRED: 'PAIRED', UNPAIRED: 'UNPAIRED' } }))
vi.mock('@/lib/strutil', () => ({ randomUUID: () => `view-${++fixture.nextId}` }))

beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  fixture.listeners.clear()
  fixture.nextId = 0
  fixture.request.mockImplementation(async (document: string) => ({ data: document === 'nearbyDevices' ? { nearbyDevices: [] } : {} }))
  fixture.bind.mockReturnValue(fixture.request)
})
afterEach(() => vi.useRealTimers())

it('acquires separate view sessions and releases each without global stop or polling', async () => {
  vi.useFakeTimers()
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const first = useDeviceDiscovery()
  const second = useDeviceDiscovery()
  await first.start()
  await first.start()
  await second.start()
  expect(fixture.request.mock.calls.filter(([document]) => document === 'acquireDiscovery').map(([, variables]) => variables.token)).toEqual(['view-1', 'view-2'])
  await first.stop()
  await first.stop()
  expect(fixture.request.mock.calls.filter(([document]) => document === 'releaseDiscovery').map(([, variables]) => variables.token)).toEqual(['view-1'])
  await second.stop()
  expect(fixture.request.mock.calls.filter(([document]) => document === 'releaseDiscovery')).toHaveLength(2)
  expect(vi.getTimerCount()).toBe(0)
})

it('balances an in-flight old view against its original transport before starting another', async () => {
  let finish!: () => void
  const blocked = new Promise<void>((resolve) => { finish = resolve })
  const oldClient = vi.fn(async (document: string) => {
    if (document === 'acquireDiscovery') await blocked
    return { data: {} }
  })
  const newClient = vi.fn(async (document: string) => ({ data: document === 'nearbyDevices' ? { nearbyDevices: [] } : {} }))
  fixture.bind.mockReturnValueOnce(oldClient).mockReturnValueOnce(newClient)
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const view = useDeviceDiscovery()
  const opening = view.start()
  await vi.waitFor(() => expect(oldClient).toHaveBeenCalledTimes(1))
  const closing = view.stop()
  const reopened = view.start()
  finish()
  await Promise.all([opening, closing, reopened])
  expect(oldClient.mock.calls.map(([document]) => document)).toEqual(['acquireDiscovery', 'releaseDiscovery'])
  expect(newClient.mock.calls.map(([document]) => document)).toEqual(['acquireDiscovery', 'nearbyDevices'])
  await view.stop()
})

it('reconnect restores only active views without an interval that resurrects closed views', async () => {
  const { useDeviceDiscovery } = await import('../../src/hooks/use-device-discovery')
  const first = useDeviceDiscovery()
  const second = useDeviceDiscovery()
  await first.start()
  await second.start()
  await first.stop()
  fixture.listeners.get('app_socket_connection_changed')!(true)
  await vi.waitFor(() => expect(fixture.request.mock.calls.filter(([document]) => document === 'acquireDiscovery')).toHaveLength(3))
  expect(fixture.request.mock.calls.filter(([document]) => document === 'acquireDiscovery').at(-1)![1].token).toBe('view-2')
  await second.stop()
})
