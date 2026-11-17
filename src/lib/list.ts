import { AccessErrorCode } from '@/lib/api/access-error-code'
import type { AccessFeature } from '@/lib/interfaces'

export function noDataKey(loading: boolean, access: readonly AccessFeature[] = [], feature: AccessFeature | '' = '', online = true): string {
  if (loading) {
    return 'loading'
  }

  if (!online) {
    return 'offline'
  }

  if (feature && !access.includes(feature)) {
    return AccessErrorCode.NoPermission
  }

  return 'no_data'
}
