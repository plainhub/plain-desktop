import { describe, expect, it } from 'vitest'
import { DeviceFeature } from '@/lib/data'
import { hasFeature, hasMediaTrash } from '@/lib/feature'

describe('hasFeature', () => {
  it('reflects the server-declared feature list', () => {
    expect(hasFeature(DeviceFeature.MEDIA_TRASH, ['MEDIA_TRASH', 'MIRROR_AUDIO'])).toBe(true)
    expect(hasFeature(DeviceFeature.MIRROR_AUDIO, ['MEDIA_TRASH'])).toBe(false)
    expect(hasFeature(DeviceFeature.MIRROR_AUDIO, [])).toBe(false)
  })

  it('reports undeclared features as unavailable', () => {
    expect(hasFeature('unknown' as DeviceFeature, ['MEDIA_TRASH'])).toBe(false)
  })

  it.each([DeviceFeature.IMAGE_SEARCH, DeviceFeature.MEDIA_SCAN])('checks %s against server capabilities', (feature) => {
    expect(hasFeature(feature, [feature])).toBe(true)
    expect(hasFeature(feature, ['MEDIA_TRASH'])).toBe(false)
    expect(hasFeature(feature, [])).toBe(false)
    expect(hasFeature(feature, undefined)).toBe(false)
  })
})

describe('hasMediaTrash', () => {
  it('follows the declared MEDIA_TRASH capability', () => {
    expect(hasMediaTrash({ features: ['MEDIA_TRASH'] })).toBe(true)
    expect(hasMediaTrash({ features: [] })).toBe(false)
    expect(hasMediaTrash({})).toBe(false)
    expect(hasMediaTrash(undefined)).toBe(false)
  })
})
