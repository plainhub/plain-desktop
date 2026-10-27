import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ context: 'local', fetch: vi.fn(), write: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({
  getGraphqlContextKey: () => mock.context,
  gqlFetch: mock.fetch,
  bindGraphqlClient: () => {
    const context = mock.context
    return (query: string, variables: unknown) => mock.write(context, query, variables)
  },
}))

beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  mock.context = 'local'
})
afterEach(() => vi.restoreAllMocks())

it('loads and replaces snapshots without retaining deleted settings', async () => {
  const core = await import('@/lib/prefs/core')
  mock.fetch.mockResolvedValue({ data: { prefs: { locale: 'fr', dark_theme: 'ON' } } })
  await core.preload()
  expect(mock.fetch).toHaveBeenCalledWith('query { prefs }', undefined, { fresh: true })
  expect(core.get('locale', '')).toBe('fr')
  core.applySnapshot({ dark_theme: 'OFF' })
  expect(core.get('locale', 'en')).toBe('en')
})

it('rejects malformed snapshots and GraphQL errors without replacing valid settings', async () => {
  const core = await import('@/lib/prefs/core')
  core.applySnapshot({ locale: 'fr' })
  for (const prefs of [null, undefined, []]) {
    mock.fetch.mockResolvedValueOnce({ data: { prefs } })
    await expect(core.preload()).rejects.toThrow('Invalid preference snapshot')
  }
  mock.fetch.mockResolvedValueOnce({ errors: [{ message: 'denied' }] })
  await expect(core.preload()).rejects.toThrow('denied')
  expect(core.get('locale', '')).toBe('fr')
})

it('serializes writes, binds queued targets and never publishes old-target values', async () => {
  const core = await import('@/lib/prefs/core')
  let finish!: (value: unknown) => void
  mock.write.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    .mockResolvedValue({ data: { result: true } })
  const first = core.set('locale', 'fr')
  const queued = core.set('dark_theme', 'ON')
  await Promise.resolve()
  expect(mock.write).toHaveBeenCalledTimes(1)
  mock.context = 'remote'
  core.applySnapshot({ locale: 'ja', dark_theme: 'OFF' })
  finish({ data: { result: true } })
  await Promise.all([first, queued])
  expect(mock.write.mock.calls.map(call => call[0])).toEqual(['local', 'local'])
  expect(core.get('locale', '')).toBe('ja')
  expect(core.get('dark_theme', '')).toBe('OFF')
  await core.set('locale', 'de')
  expect(mock.write.mock.calls[2][0]).toBe('remote')
  expect(core.get('locale', '')).toBe('de')
})

it('invalidates cached preferences when the target or credentials change', async () => {
  const core = await import('@/lib/prefs/core')
  core.applySnapshot({ locale: 'fr' })
  mock.context = 'remote-session'
  expect(core.get('locale', 'en')).toBe('en')
  core.applySnapshot({ locale: 'ja' })
  mock.context = 'remote-rekeyed'
  expect(core.get('locale', 'en')).toBe('en')
})

it('keeps failed writes visible and processes subsequent writes', async () => {
  const core = await import('@/lib/prefs/core')
  vi.spyOn(console, 'error').mockImplementation(() => {})
  core.applySnapshot({ locale: 'ja' })
  mock.write.mockResolvedValueOnce({ errors: [{ message: 'denied' }] })
    .mockResolvedValueOnce({ data: { result: false } })
    .mockResolvedValueOnce({ data: { result: true } })
  await expect(core.set('locale', 'fr')).rejects.toThrow('denied')
  await expect(core.set('locale', 'de')).rejects.toThrow('Preference write refused')
  expect(core.get('locale', '')).toBe('ja')
  await core.set('locale', 'en')
  expect(core.get('locale', '')).toBe('en')
})
