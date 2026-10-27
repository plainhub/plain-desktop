import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { loadDeviceIdentity } from '@/lib/device/identity'

const mock = vi.hoisted(() => ({ local: true, native: vi.fn(), init: vi.fn(), query: vi.fn() }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => mock.local }))
vi.mock('@/lib/device/native-settings', () => ({ readNativeDeviceIdentity: mock.native }))
vi.mock('@/lib/api/init', () => ({ requestInit: mock.init }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: mock.query }))
beforeEach(() => {
  vi.resetAllMocks()
  vi.stubGlobal('__IS_TAURI__', true)
  mock.query.mockResolvedValue({ data: { app: { clientId: 'target', deviceName: 'Device' } } })
})
afterEach(() => vi.unstubAllGlobals())

it.each([true, false])('reads the key from the queried target with localMode=%s', async (local) => {
  mock.local = local
  mock.native.mockResolvedValue({ signaturePublicKey: 'native-key' })
  mock.init.mockResolvedValue({ data: { signaturePublicKey: 'remote-key' } })
  expect((await loadDeviceIdentity()).publicKey).toBe(local ? 'native-key' : 'remote-key')
  expect(mock.native).toHaveBeenCalledTimes(local ? 1 : 0)
  expect(mock.init).toHaveBeenCalledTimes(local ? 0 : 1)
})
