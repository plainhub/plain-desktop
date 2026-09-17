import { describe, expect, it } from 'vitest'
import { FEATURE } from '@/lib/data'
import { hasFeature, hasMediaTrash } from '@/lib/feature'
import { DeviceType } from '@/lib/status'

const R = 30 // Android 11, first release where MEDIA_TRASH became available

describe('hasMediaTrash', () => {
  it('is always available on a NAS, whatever the reported osVersion', () => {
    for (const osVersion of [0, 28, R, 34]) {
      expect(hasMediaTrash({ deviceType: DeviceType.NAS, osVersion })).toBe(true)
    }
  })

  it.each([DeviceType.PHONE, DeviceType.TABLET, DeviceType.COMPUTER, DeviceType.TV, DeviceType.OTHER])(
    'follows the Android R+ gate on %s',
    (deviceType) => {
      expect(hasMediaTrash({ deviceType, osVersion: R - 1 })).toBe(false)
      expect(hasMediaTrash({ deviceType, osVersion: R })).toBe(true)
    },
  )
})

describe('hasFeature', () => {
  it('gates MEDIA_TRASH on Android R+', () => {
    expect(hasFeature(FEATURE.MEDIA_TRASH, R - 1)).toBe(false)
    expect(hasFeature(FEATURE.MEDIA_TRASH, R)).toBe(true)
  })

  it('gates MIRROR_AUDIO on Android Q+', () => {
    expect(hasFeature(FEATURE.MIRROR_AUDIO, 28)).toBe(false)
    expect(hasFeature(FEATURE.MIRROR_AUDIO, 29)).toBe(true)
  })

  it('reports unknown features as unavailable', () => {
    expect(hasFeature('unknown' as FEATURE, 34)).toBe(false)
  })
})
