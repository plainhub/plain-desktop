import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import * as browser from '@/lib/prefs/browser'
import * as appearance from '@/lib/prefs/appearance'
import { getCurrentMode, changeColor } from '@/lib/theme'

const mock = vi.hoisted(() => ({ get: vi.fn(), set: vi.fn(), load: vi.fn(), sync: vi.fn() }))
vi.mock('@/lib/prefs/core', () => ({ get: mock.get, set: mock.set }))
vi.mock('@/plugins/i18n', () => ({ loadLocaleMessages: mock.load, syncLocaleFromPrefs: mock.sync }))
vi.mock('@/plugins/eventbus.js', () => ({ default: { emit: vi.fn() } }))

let storage: Map<string, string>
beforeEach(() => {
  vi.resetAllMocks()
  vi.stubGlobal('__IS_TAURI__', false)
  storage = new Map()
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
    clear: () => storage.clear(),
  })
  vi.stubGlobal('document', { documentElement: { classList: { add: vi.fn(), remove: vi.fn() } } })
  vi.stubGlobal('window', { matchMedia: () => ({ matches: false }) })
})
afterEach(() => vi.unstubAllGlobals())

it.each([['USE_DEVICE_THEME', 'auto'], ['ON', 'dark'], ['OFF', 'light']] as const)('keeps browser theme %s and language independent of Core preferences', async (value, mode) => {
  mock.get.mockReturnValue('ON')
  await appearance.set('dark_theme', value)
  await appearance.set('locale', 'fr')
  expect(storage.get('dark_theme')).toBe(value)
  await appearance.apply()
  expect(getCurrentMode()).toBe(mode)
  expect(mode === 'dark' ? document.documentElement.classList.add : document.documentElement.classList.remove).toHaveBeenCalledWith('dark')
  expect(mock.load).toHaveBeenCalledWith('fr')
  expect(mock.get).not.toHaveBeenCalled()
  expect(mock.set).not.toHaveBeenCalled()
  browser.clear(appearance.APPEARANCE_KEYS)
  expect(appearance.get('locale', '')).toBe('fr')
})

it('uses Desktop Core preferences without reading browser appearance values', async () => {
  vi.stubGlobal('__IS_TAURI__', true)
  storage.set('dark_theme', 'OFF')
  mock.get.mockReturnValue('ON')
  mock.set.mockResolvedValue(undefined)
  expect(getCurrentMode()).toBe('dark')
  await appearance.set('locale', 'ja')
  expect(mock.set).toHaveBeenCalledWith('locale', 'ja')
  expect(storage.has('locale')).toBe(false)
})

it('recomputes automatic theme from the system without persisting its resolved color', async () => {
  await appearance.set('dark_theme', 'USE_DEVICE_THEME')
  const saved = [...storage]
  let dark = false
  vi.stubGlobal('window', { matchMedia: () => ({ matches: dark }) })
  changeColor()
  expect(document.documentElement.classList.remove).toHaveBeenCalledWith('dark')
  dark = true
  changeColor()
  expect(document.documentElement.classList.add).toHaveBeenCalledWith('dark')
  expect([...storage]).toEqual(saved)
  expect(mock.set).not.toHaveBeenCalled()
})
