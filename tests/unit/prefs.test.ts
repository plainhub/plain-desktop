import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const { native, remote, invoke } = vi.hoisted(() => ({ native: vi.fn(), remote: vi.fn(), invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: native }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: remote }))

beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  vi.stubGlobal('__IS_TAURI__', true)
})
afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

it('preloads synchronous readers from native preferences', async () => {
  const prefs = await import('../../src/lib/prefs')
  invoke.mockResolvedValue({ client_id: 'local-core', locale: 'fr', 'main_state:peer': { selected: 'files' } })
  await prefs.preload()
  expect(invoke).toHaveBeenCalledWith('native_system_preferences')
  expect(prefs.get('client_id', '')).toBe('local-core')
  expect(prefs.get('locale', '')).toBe('fr')
  expect(prefs.get('main_state:peer', null)).toEqual({ selected: 'files' })
  expect(prefs.get('absent', 'fallback')).toBe('fallback')
  expect(remote).not.toHaveBeenCalled()
})

it('serializes optimistic writes and deletes through existing main mutations', async () => {
  const prefs = await import('../../src/lib/prefs')
  let complete!: (value: unknown) => void
  native.mockImplementationOnce(() => new Promise((resolve) => { complete = resolve }))
    .mockResolvedValue({ data: { result: { affectedCount: 1 } } })
  const first = prefs.set('main_state:peer', { selected: 'chat' })
  const second = prefs.remove('main_state:peer')
  expect(prefs.get('main_state:peer', null)).toBeNull()
  await Promise.resolve()
  expect(native).toHaveBeenCalledTimes(1)
  expect(native.mock.calls[0][0]).toMatchObject({ variables: { key: 'main_state:peer', value: { selected: 'chat' } } })
  expect(native.mock.calls[0][0].query).toContain('setSystemPref(key: $key, value: $value)')
  complete({ data: { result: { affectedCount: 1 } } })
  await Promise.all([first, second])
  expect(native).toHaveBeenCalledTimes(2)
  expect(native.mock.calls[1][0].query).toContain('removeSystemPref(key: $key)')
  expect(native.mock.calls[1][0].variables).toEqual({ key: 'main_state:peer' })
  expect(remote).not.toHaveBeenCalled()
})

it('reports rejected writes and continues processing later writes', async () => {
  const prefs = await import('../../src/lib/prefs')
  const report = vi.spyOn(console, 'error').mockImplementation(() => {})
  native.mockResolvedValueOnce({ errors: [{ message: 'no_permission' }] })
    .mockResolvedValueOnce({ data: { result: { affectedCount: 1 } } })
  const first = prefs.set('locale', 'fr')
  const second = prefs.set('locale', 'en')
  await expect(first).rejects.toThrow('no_permission')
  await second
  expect(report).toHaveBeenCalledTimes(1)
  expect(native).toHaveBeenCalledTimes(2)
  expect(prefs.get('locale', '')).toBe('en')
})

it('rejects unreadable startup snapshots instead of inventing a local identity', async () => {
  const prefs = await import('../../src/lib/prefs')
  for (const response of [null, undefined, []]) {
    invoke.mockResolvedValueOnce(response)
    await expect(prefs.preload()).rejects.toThrow()
  }
  expect(prefs.get('client_id', '')).toBe('')
})

it('keeps browser storage and logout independent of the native core', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  const storage = new Map<string, string>()
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
    clear: () => storage.clear(),
  })
  const prefs = await import('../../src/lib/prefs')
  await prefs.preload()
  await prefs.set('locale', 'fr')
  await prefs.set('main_state:peer', { selected: 'files' })
  expect(prefs.get('locale', '')).toBe('fr')
  expect(prefs.get('main_state:peer', null)).toEqual({ selected: 'files' })
  await prefs.remove('locale')
  expect(prefs.get('locale', 'en')).toBe('en')
  prefs.clearWebPreferences()
  expect(storage.size).toBe(0)
  expect(native).not.toHaveBeenCalled()
})
