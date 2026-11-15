import { AppFeatureType } from '@/lib/app-feature.generated'
export const DEFAULT_HOME_FEATURES = [
  AppFeatureType.AUDIO,
  AppFeatureType.IMAGES,
  AppFeatureType.VIDEOS,
  AppFeatureType.DOCS,
  AppFeatureType.FILES,
  AppFeatureType.APPS,
  AppFeatureType.NOTES,
  AppFeatureType.FEEDS,
  AppFeatureType.SMS,
  AppFeatureType.CALLS,
  AppFeatureType.CONTACTS,
  AppFeatureType.SCREEN_MIRROR,
  AppFeatureType.IMAGE_EDITOR,
  AppFeatureType.CALL_PHONE,
]

export function normalizeHomeFeatures(ids: AppFeatureType[], availableIds: AppFeatureType[]): AppFeatureType[] {
  const availableSet = new Set(availableIds)
  const result: AppFeatureType[] = []
  const seen = new Set<AppFeatureType>()

  for (const id of ids) {
    if (!availableSet.has(id) || seen.has(id)) continue
    seen.add(id)
    result.push(id)
  }

  if (result.length === 0) {
    for (const id of DEFAULT_HOME_FEATURES) {
      if (!availableSet.has(id) || seen.has(id)) continue
      seen.add(id)
      result.push(id)
    }
  }

  return result
}
