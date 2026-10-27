import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({
  mounted: [] as Array<() => void>,
  unmounted: [] as Array<() => void>,
  listeners: new Map<string, (value: any) => void>(),
  emit: vi.fn(),
}))
vi.mock('vue', async original => ({
  ...await original<typeof import('vue')>(),
  onMounted: (fn: () => void) => mock.mounted.push(fn),
  onUnmounted: (fn: () => void) => mock.unmounted.push(fn),
}))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (value: string) => value }) }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe: (name: string, handler: (value: any) => void) => { mock.listeners.set(name, handler); return () => mock.listeners.delete(name) },
  onConnectionChange: (handler: (value: boolean) => void) => { mock.listeners.set('connection', handler); return () => mock.listeners.delete('connection') },
} }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: mock.emit, on: vi.fn() } }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/lib/api/api', () => ({ getLocalToken: () => 'fixture', isNativeTarget: () => false, setLocalServerPort: vi.fn(), setLocalServerHttpsPort: vi.fn() }))
vi.mock('@/lib/device/login-peers', () => ({ subscribeLoginPeerUpdates: vi.fn() }))
vi.mock('@/lib/theme', () => ({ applyDarkClass: vi.fn(), changeColor: vi.fn(), changeColorMode: vi.fn(), getCurrentMode: () => 'light', getLastSavedAutoColorMode: () => 'light', isModeDark: () => false }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: () => new Uint8Array(32) }))
vi.mock('@/lib/api/native-events', () => ({ subscribeNativeEvents: vi.fn() }))
vi.mock('@/lib/prefs', () => ({ applySnapshot: vi.fn() }))

import { useAppSocket } from '../../src/hooks/app-socket'

beforeEach(() => {
  mock.mounted.length = 0
  mock.unmounted.length = 0
  mock.listeners.clear()
  mock.emit.mockClear()
  vi.useFakeTimers()
  vi.stubGlobal('document', { title: '' })
  vi.stubGlobal('window', { matchMedia: () => ({ addEventListener: vi.fn() }) })
  vi.stubGlobal('__IS_TAURI__', false)
})
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals() })

it('uses GraphQL connection status and releases listeners on unmount', async () => {
  const state = useAppSocket()
  mock.mounted.forEach(fn => fn())
  mock.listeners.get('connection')!(false)
  expect(state.wsStatus.value).toBe('closed')
  mock.listeners.get('connection')!(true)
  mock.listeners.get('connection')!(false)
  await vi.advanceTimersByTimeAsync(2000)
  expect(state.wsStatus.value).toBe('closed')
  mock.listeners.get('connection')!(true)
  await vi.advanceTimersByTimeAsync(2000)
  expect(state.wsStatus.value).toBe('')
  mock.unmounted.forEach(fn => fn())
  expect(mock.listeners.size).toBe(0)
})

it('projects authenticated pairing subscriptions to the existing UI handlers', () => {
  useAppSocket()
  mock.mounted.forEach(fn => fn())
  mock.listeners.get('pairingCompleted')!({ deviceId: 'peer', deviceName: 'Peer', ip: 'host' })
  expect(mock.emit).toHaveBeenCalledWith('PAIRING_SUCCESS', { deviceId: 'peer', deviceName: 'Peer', ip: 'host', error: '' })
  mock.listeners.get('pairingFailed')!({ deviceId: 'peer', deviceName: 'Peer', error: 'failed' })
  expect(mock.emit).toHaveBeenCalledWith('PAIRING_FAILED', { deviceId: 'peer', deviceName: 'Peer', error: 'failed' })
})
