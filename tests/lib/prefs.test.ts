import { beforeEach, describe, expect, it } from 'vitest'
import { get, set } from '@/lib/prefs'

beforeEach(() => {
  localStorage.clear()
})

describe('prefs web (localStorage) round-trip', () => {
  it('stores and reads a plain string value', () => {
    set('color-mode', 'light')
    expect(localStorage.getItem('color-mode')).toBe('light')
    expect(get('color-mode', null)).toBe('light')
    expect(get('color-mode', 'auto')).toBe('light')
  })

  it('stores and reads a JSON value (object / number / boolean)', () => {
    set('state', { a: 1 })
    expect(get<{ a: number }>('state', { a: 0 })).toEqual({ a: 1 })
    set('count', 3)
    expect(get('count', 0)).toBe(3)
    set('flag', true)
    expect(get('flag', false)).toBe(true)
  })

  it('returns the fallback when the key is absent', () => {
    expect(get('color-mode', null)).toBeNull()
    expect(get('client_id', '')).toBe('')
  })
})