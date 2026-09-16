import { describe, it, expect } from 'vitest'
import { ALL_FEATURES, NAS_FEATURE_IDS, getAvailableFeatures, withNasChatRetention } from '@/views/app-rail/features'

describe('getAvailableFeatures', () => {
  it('returns the full set for a non-NAS device', () => {
    const ids = getAvailableFeatures(false).map((f) => f.id)
    expect(ids).toEqual([
      'files', 'audios', 'images', 'videos', 'chat', 'docs', 'apps',
      'notes', 'feeds', 'messages', 'calls', 'contacts', 'screen_mirror',
    ])
  })

  it('returns only the NAS whitelist for a NAS device, in definition order', () => {
    const ids = getAvailableFeatures(true).map((f) => f.id)
    expect(ids).toEqual(['files', 'audios', 'images', 'videos', 'docs'])
    expect(ids.every((id) => NAS_FEATURE_IDS.has(id))).toBe(true)
  })

  it('never exposes requireNonGoogle on features', () => {
    for (const feature of ALL_FEATURES) {
      expect(feature).not.toHaveProperty('requireNonGoogle')
    }
  })

  it('hides debug-only features unless debug is enabled (non-NAS)', () => {
    expect(getAvailableFeatures(false, false).some((f) => f.id === 'image_editor')).toBe(false)
    expect(getAvailableFeatures(false, true).some((f) => f.id === 'image_editor')).toBe(true)
  })

  it('hides debug-only features on NAS even with debug enabled', () => {
    const ids = getAvailableFeatures(true, true).map((f) => f.id)
    expect(ids).not.toContain('image_editor')
  })

  it('force-retains chat on the NAS rail even when customization dropped it', () => {
    const available = getAvailableFeatures(true)
    const customized = available.filter((f) => ['files', 'audios'].includes(f.id))
    const retained = withNasChatRetention(customized, true)
    expect(retained.map((f) => f.id)).toEqual(['files', 'audios', 'chat'])
    // already present: no duplicate
    expect(withNasChatRetention(available, true).filter((f) => f.id === 'chat')).toHaveLength(1)
    // non-NAS rails are untouched
    expect(withNasChatRetention(available, false)).toEqual(available)
    // chat itself is filtered from NAS feature cards — it is rail-only.
    expect(NAS_FEATURE_IDS.has('chat')).toBe(false)
  })
})
