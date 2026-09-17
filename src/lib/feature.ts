import { FEATURE } from '@/lib/data'
import { isQPlus, isRPlus } from '@/lib/sdk-version'
import { DeviceType } from '@/lib/status'

export const hasFeature = (feature: FEATURE, osVersion: number) => {
  if (feature === FEATURE.MEDIA_TRASH) {
    return isRPlus(osVersion)
  } else if (feature === FEATURE.MIRROR_AUDIO) {
    return isQPlus(osVersion)
  }

  return false
}

/** Media trash needs Android R+ on a phone (scoped storage). A NAS backend
 *  implements trash at the filesystem level, so it is always available. */
export const hasMediaTrash = (app: { deviceType: DeviceType; osVersion: number }) =>
  app.deviceType === DeviceType.NAS || isRPlus(app.osVersion)
