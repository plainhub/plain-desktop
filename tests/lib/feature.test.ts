import { describe, expect, it } from 'vitest'
import { FEATURE } from '@/lib/data'
import { hasFeature, hasMediaTrash } from '@/lib/feature'

describe('hasFeature', () => {
  it('reflects the server-declared feature list', () => {
    expect(hasFeature(FEATURE.MEDIA_TRASH, ['MEDIA_TRASH', 'MIRROR_AUDIO'])).toBe(true)
    expect(hasFeature(FEATURE.MIRROR_AUDIO, ['MEDIA_TRASH'])).toBe(false)
    expect(hasFeature(FEATURE.MIRROR_AUDIO, [])).toBe(false)
  })

  it('reports undeclared features as unavailable', () => {
    expect(hasFeature('unknown' as FEATURE, ['MEDIA_TRASH'])).toBe(false)
  })
})

describe('hasMediaTrash', () => {
  it('follows the declared MEDIA_TRASH capability', () => {
    expect(hasMediaTrash({ features: ['MEDIA_TRASH'] })).toBe(true)
    expect(hasMediaTrash({ features: [] })).toBe(false)
  })
})
