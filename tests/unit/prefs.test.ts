import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const { native, remote, invoke } = vi.hoisted(() => ({ native: vi.fn(), remote: vi.fn(), invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: native }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: remote }))
vi.mock('@/plugins/i18n', () => ({ loadLocaleMessages: vi.fn(), syncLocaleFromPrefs: vi.fn() }))

let storage: Map<string, string>
beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  vi.stubGlobal('__IS_TAURI__', true)
  storage = new Map()
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
    clear: () => storage.clear(),
  })
})
afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

it.each([
  ['USE_DEVICE_THEME', 'auto'],
  ['ON', 'dark'],
  ['OFF', 'light'],
])('applies the Rust theme name %s on desktop and web', async (value, mode) => {
  const prefs = await import('../../src/lib/prefs')
  const theme = await import('../../src/lib/theme')
  native.mockResolvedValue({ data: { prefs: { dark_theme: value } } })
  invoke.mockResolvedValue({ clientId: 'local-core', signaturePublicKey: 'public-key' })
  await prefs.preload()
  expect(theme.getCurrentMode()).toBe(mode)

  vi.stubGlobal('__IS_TAURI__', false)
  const classList = { add: vi.fn(), remove: vi.fn() }
  vi.stubGlobal('document', { documentElement: { classList } })
  vi.stubGlobal('window', { matchMedia: () => ({ matches: false }) })
  await prefs.applySnapshot({ dark_theme: value })
  expect(theme.getCurrentMode()).toBe(mode)
  expect(mode === 'dark' ? classList.add : classList.remove).toHaveBeenCalledWith('dark')
})

it('loads filtered prefs and obtains core identity through its dedicated native API', async () => {
  const prefs = await import('../../src/lib/prefs')
  native.mockResolvedValue({ data: { prefs: { locale: 'fr', dark_theme: 'ON' } } })
  invoke.mockResolvedValue({ clientId: 'local-core', signaturePublicKey: 'public-key' })
  await prefs.preload()
  expect(native).toHaveBeenCalledWith({ query: 'query { prefs }', variables: undefined }, 30000)
  expect(invoke).toHaveBeenCalledWith('native_device_identity')
  expect(prefs.get('client_id', '')).toBe('local-core')
  expect(prefs.get('locale', '')).toBe('fr')
  expect(prefs.get('dark_theme', 'USE_DEVICE_THEME')).toBe('ON')
  expect(prefs.get('absent', 'fallback')).toBe('fallback')
  expect(remote).not.toHaveBeenCalled()
})

it('serializes preference writes and deletes and publishes only successful results', async () => {
  const prefs = await import('../../src/lib/prefs')
  let complete!: (value: unknown) => void
  native.mockImplementationOnce(() => new Promise((resolve) => { complete = resolve }))
    .mockResolvedValue({ data: { result: true } })
  const first = prefs.set('dark_theme', 'ON')
  const second = prefs.remove('dark_theme')
  expect(prefs.get('dark_theme', 'USE_DEVICE_THEME')).toBe('USE_DEVICE_THEME')
  await Promise.resolve()
  expect(native).toHaveBeenCalledTimes(1)
  expect(native.mock.calls[0][0]).toMatchObject({ variables: { key: 'dark_theme', value: 'ON' } })
  expect(native.mock.calls[0][0].query).toContain('setPref(key: $key, value: $value)')
  complete({ data: { result: true } })
  await Promise.all([first, second])
  expect(native).toHaveBeenCalledTimes(2)
  expect(native.mock.calls[1][0].query).toContain('removePref(key: $key)')
  expect(native.mock.calls[1][0].variables).toEqual({ key: 'dark_theme' })
  expect(prefs.get('dark_theme', 'USE_DEVICE_THEME')).toBe('USE_DEVICE_THEME')
  expect(remote).not.toHaveBeenCalled()
})

it('reports rejected writes and continues processing later writes', async () => {
  const prefs = await import('../../src/lib/prefs')
  const report = vi.spyOn(console, 'error').mockImplementation(() => {})
  native.mockResolvedValueOnce({ errors: [{ message: 'no_permission' }] })
    .mockResolvedValueOnce({ data: { result: true } })
  const first = prefs.set('locale', 'fr')
  const second = prefs.set('locale', 'en')
  await expect(first).rejects.toThrow('no_permission')
  await second
  expect(report).toHaveBeenCalledTimes(1)
  expect(native).toHaveBeenCalledTimes(2)
  expect(prefs.get('locale', '')).toBe('en')
})

it('rejects unreadable preference snapshots instead of inventing a local identity', async () => {
  const prefs = await import('../../src/lib/prefs')
  invoke.mockResolvedValue({ clientId: 'local-core', signaturePublicKey: 'public-key' })
  for (const snapshot of [null, undefined, []]) {
    native.mockResolvedValueOnce({ data: { prefs: snapshot } })
    await expect(prefs.preload()).rejects.toThrow('Invalid preference snapshot')
  }
  expect(prefs.get('client_id', '')).toBe('')
})

it('stores frontend-only UI state locally rather than sending arbitrary native pref keys', async () => {
  const prefs = await import('../../src/lib/prefs')
  await prefs.set('main_state:peer', { selected: 'files' })
  expect(prefs.get('main_state:peer', null)).toEqual({ selected: 'files' })
  await prefs.remove('main_state:peer')
  expect(prefs.get('main_state:peer', null)).toBeNull()
  expect(native).not.toHaveBeenCalled()
  expect(remote).not.toHaveBeenCalled()
})

it('uses remote prefs for browser settings while logout clears only browser UI storage', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  remote.mockResolvedValue({ data: { result: true } })
  const prefs = await import('../../src/lib/prefs')
  await prefs.preload()
  await prefs.set('locale', 'fr')
  await prefs.set('main_state:peer', { selected: 'files' })
  expect(prefs.get('locale', '')).toBe('fr')
  expect(storage.has('locale')).toBe(false)
  expect(prefs.get('main_state:peer', null)).toEqual({ selected: 'files' })
  expect(remote.mock.calls[0][0]).toContain('setPref(key: $key, value: $value)')
  await prefs.remove('locale')
  expect(prefs.get('locale', 'en')).toBe('en')
  expect(remote.mock.calls[1][0]).toContain('removePref(key: $key)')
  prefs.clearWebPreferences()
  expect(storage.size).toBe(0)
  expect(native).not.toHaveBeenCalled()
  expect(invoke).not.toHaveBeenCalled()
})
