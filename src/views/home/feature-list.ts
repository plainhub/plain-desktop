export const DEFAULT_HOME_FEATURES = [
  'audios',
  'images',
  'videos',
  'docs',
  'files',
  'apps',
  'notes',
  'feeds',
  'messages',
  'calls',
  'contacts',
  'screen_mirror',
  'image_editor',
  'clipboard',
  'call_phone',
]

export function normalizeHomeFeatures(ids: string[], availableIds: string[]): string[] {
  const availableSet = new Set(availableIds)
  const result: string[] = []
  const seen = new Set<string>()

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
