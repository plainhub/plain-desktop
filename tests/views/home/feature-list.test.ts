import { describe, expect, it } from 'vitest'
import { DEFAULT_HOME_FEATURES, normalizeHomeFeatures } from '@/views/home/feature-list'

describe('normalizeHomeFeatures', () => {
  it('keeps user order and omits disabled features', () => {
    const ids = ['contacts', 'messages', 'audios']
    expect(normalizeHomeFeatures(ids, DEFAULT_HOME_FEATURES)).toEqual(ids)
  })

  it('does not re-append disabled default features', () => {
    const available = [...DEFAULT_HOME_FEATURES]
    const withoutContacts = DEFAULT_HOME_FEATURES.filter((id) => id !== 'contacts')
    expect(normalizeHomeFeatures(withoutContacts, available)).toEqual(withoutContacts)
  })

  it('drops unavailable and duplicate ids', () => {
    expect(normalizeHomeFeatures(['files', 'files', 'debug_only', 'notes'], ['files', 'notes'])).toEqual([
      'files',
      'notes',
    ])
  })

  it('falls back to available defaults when result is empty', () => {
    expect(normalizeHomeFeatures([], ['notes', 'feeds', 'off_default'])).toEqual(['notes', 'feeds'])
  })

  it('falls back to defaults when every stored id is unavailable', () => {
    expect(normalizeHomeFeatures(['gone_a', 'gone_b'], ['notes'])).toEqual(['notes'])
  })
})
