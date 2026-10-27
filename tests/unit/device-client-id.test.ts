import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ native: vi.fn(), uuid: vi.fn() }))
vi.mock('@/lib/device/native-settings', () => ({ readNativeDeviceIdentity: mock.native }))
vi.mock('@/lib/strutil', () => ({ shortUUID: mock.uuid }))
let storage: Map<string, string>
beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  vi.stubGlobal('__IS_TAURI__', true)
  storage = new Map()
  const session = new Map<string, string>()
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
  })
  vi.stubGlobal('sessionStorage', {
    getItem: (key: string) => session.get(key) ?? null,
    setItem: (key: string, value: string) => session.set(key, value),
    removeItem: (key: string) => session.delete(key),
  })
})
afterEach(() => vi.unstubAllGlobals())

it('loads native identity separately from preferences and keeps it when binding a remote target', async () => {
  const identity = await import('@/lib/device/client-id')
  storage.set('client_id', 'browser')
  mock.native.mockResolvedValue({ clientId: 'native', signaturePublicKey: 'key' })
  expect(identity.getDesktopClientId()).toBe('')
  await identity.preloadClientIdentity()
  identity.setRemoteClientId('remote')
  expect(identity.getDesktopClientId()).toBe('native')
  expect(identity.getActiveClientId()).toBe('remote')
  identity.clearRemoteClientId()
  expect(identity.getActiveClientId()).toBe('native')
})

it('creates browser identity once and reuses it without native access', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  mock.uuid.mockReturnValue('browser')
  const identity = await import('@/lib/device/client-id')
  identity.ensureBrowserClientId()
  identity.ensureBrowserClientId()
  expect(identity.getDesktopClientId()).toBe('browser')
  expect(mock.uuid).toHaveBeenCalledTimes(1)
  expect(mock.native).not.toHaveBeenCalled()
})
