import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ native: vi.fn(), uuid: vi.fn() }))
vi.mock('@/lib/device/native-settings', () => ({ readNativeDeviceIdentity: mock.native }))
vi.mock('@/lib/device/client-identity.generated', () => ({ generateClientId: mock.uuid }))
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
    clear: () => storage.clear(),
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
  mock.uuid.mockReturnValue('706c6169-0000-8000-8000-000000000001')
  const identity = await import('@/lib/device/client-id')
  expect(identity.ensureBrowserClientId()).toBe(false)
  expect(identity.ensureBrowserClientId()).toBe(true)
  expect(identity.getDesktopClientId()).toBe('706c6169-0000-8000-8000-000000000001')
  expect(mock.uuid).toHaveBeenCalledTimes(1)
  expect(mock.native).not.toHaveBeenCalled()
})

it.each([
  'legacy-client',
  '',
  '706c6169000080008000000000000001',
  '706c6169-0000-8000-8000-00000000000z',
  ' 706c6169-0000-8000-8000-000000000001',
  '706c6169-0000-8000-8000-000000000001\n',
])(
  'clears browser storage and device binding for invalid client identity %j',
  async (clientId) => {
    vi.stubGlobal('__IS_TAURI__', false)
    const replacement = '706c6169-0000-8000-8000-000000000002'
    mock.uuid.mockReturnValue(replacement)
    storage.set('client_id', clientId)
    storage.set('auth_token', 'expired-session')
    storage.set('login_identity', 'old-identity')
    storage.set('locale', 'fr')
    const identity = await import('@/lib/device/client-id')
    identity.setRemoteClientId('old-device')

    expect(identity.ensureBrowserClientId()).toBe(false)
    expect([...storage.entries()]).toEqual([['client_id', replacement]])
    expect(identity.getRemoteClientId()).toBe('')
    expect(identity.getActiveClientId()).toBe(replacement)
    expect(identity.ensureBrowserClientId()).toBe(true)
    expect(mock.uuid).toHaveBeenCalledTimes(1)
    expect(mock.native).not.toHaveBeenCalled()
  },
)

it.each(['706c6169-0000-8000-8000-000000000001', '706C6169-0000-8000-8000-000000000001'])(
  'preserves valid UUID identity %s and browser session',
  async (clientId) => {
    vi.stubGlobal('__IS_TAURI__', false)
    storage.set('client_id', clientId)
    storage.set('auth_token', 'current-session')
    storage.set('locale', 'fr')
    const before = [...storage.entries()]
    const identity = await import('@/lib/device/client-id')

    expect(identity.ensureBrowserClientId()).toBe(true)
    expect([...storage.entries()]).toEqual(before)
    expect(mock.uuid).not.toHaveBeenCalled()
    expect(mock.native).not.toHaveBeenCalled()
  },
)
