import { describe, expect, it } from 'vitest'
import { Capability } from '@/lib/data'
import { hasFeature, hasMediaTrash } from '@/lib/feature'

describe('hasFeature', () => {
  it('reflects the server-declared feature list', () => {
    expect(hasFeature(Capability.MEDIA_TRASH, ['MEDIA_TRASH', 'MIRROR_AUDIO'])).toBe(true)
    expect(hasFeature(Capability.MIRROR_AUDIO, ['MEDIA_TRASH'])).toBe(false)
    expect(hasFeature(Capability.MIRROR_AUDIO, [])).toBe(false)
  })

  it('reports undeclared features as unavailable', () => {
    expect(hasFeature('unknown' as Capability, ['MEDIA_TRASH'])).toBe(false)
  })

  it.each([Capability.IMAGE_SEARCH, Capability.MEDIA_SCAN])('checks %s against server capabilities', (feature) => {
    expect(hasFeature(feature, [feature])).toBe(true)
    expect(hasFeature(feature, ['MEDIA_TRASH'])).toBe(false)
    expect(hasFeature(feature, [])).toBe(false)
    expect(hasFeature(feature, undefined)).toBe(false)
  })
})

describe('hasMediaTrash', () => {
  it('follows the declared MEDIA_TRASH capability', () => {
    expect(hasMediaTrash({ capabilities: ['MEDIA_TRASH'] })).toBe(true)
    expect(hasMediaTrash({ capabilities: [] })).toBe(false)
    expect(hasMediaTrash({})).toBe(false)
    expect(hasMediaTrash(undefined)).toBe(false)
  })
})
