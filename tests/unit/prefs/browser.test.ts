import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import * as browser from '@/lib/prefs/browser'

let storage: Map<string, string>
beforeEach(() => {
  storage = new Map()
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
    clear: () => storage.clear(),
  })
})
afterEach(() => vi.unstubAllGlobals())

it('round trips strings and structured UI state and removes individual keys', () => {
  browser.set('locale', 'fr')
  browser.set('main_state:peer', { selected: 'files' })
  expect(browser.get('locale', '')).toBe('fr')
  expect(browser.get('main_state:peer', null)).toEqual({ selected: 'files' })
  browser.remove('main_state:peer')
  expect(browser.get('main_state:peer', null)).toBeNull()
  storage.set('invalid-json', '{')
  expect(browser.get('invalid-json', null)).toBe('{')
})

it('clears sessions and UI state while retaining only the requested appearance keys', () => {
  for (const [key, value] of Object.entries({ locale: 'fr', dark_theme: 'OFF', auth_token: 'session', client_id: 'browser', main_state: '{}' })) browser.set(key, value)
  browser.clear(['locale', 'dark_theme'])
  expect([...storage]).toEqual([['locale', 'fr'], ['dark_theme', 'OFF']])
  browser.clear()
  expect(storage.size).toBe(0)
})

it('propagates storage failures to its caller', () => {
  vi.stubGlobal('localStorage', { setItem: () => { throw new Error('quota') } })
  expect(() => browser.set('locale', 'fr')).toThrow('quota')
})
