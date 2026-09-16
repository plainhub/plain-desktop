import { describe, it, expect } from 'vitest'
import { getAvailableHomeFeatures } from '@/views/home/features'

describe('getAvailableHomeFeatures', () => {
  it('returns exactly the five NAS cards in order for a NAS device', () => {
    const features = getAvailableHomeFeatures(true)
    expect(features.map((f) => f.id)).toEqual(['audios', 'images', 'videos', 'docs', 'files'])
    expect(features.every((f) => f.sectionType === 'feature')).toBe(true)
  })

  it('omits the call phone panel on NAS', () => {
    const ids = getAvailableHomeFeatures(true).map((f) => f.id)
    expect(ids).not.toContain('call_phone')
  })

  it('maps count keys for the NAS cards and leaves files without one', () => {
    const byId = new Map(getAvailableHomeFeatures(true).map((f) => [f.id, f]))
    expect(byId.get('audios')?.countKey).toBe('audios')
    expect(byId.get('images')?.countKey).toBe('images')
    expect(byId.get('videos')?.countKey).toBe('videos')
    expect(byId.get('docs')?.countKey).toBe('docs')
    expect(byId.get('files')?.countKey).toBeUndefined()
  })

  it('returns the full home set with phone panels for a non-NAS device', () => {
    const ids = getAvailableHomeFeatures(false).map((f) => f.id)
    expect(ids).toContain('apps')
    expect(ids).toContain('notes')
    expect(ids).toContain('feeds')
    expect(ids).not.toContain('clipboard')
    expect(ids).toContain('call_phone')
    expect(ids[0]).toBe('audios')
  })

  it('hides debug-only features unless debug is enabled (non-NAS)', () => {
    const withoutDebug = getAvailableHomeFeatures(false, false).map((f) => f.id)
    const withDebug = getAvailableHomeFeatures(false, true).map((f) => f.id)
    expect(withoutDebug).not.toContain('image_editor')
    expect(withDebug).toContain('image_editor')
  })
})
