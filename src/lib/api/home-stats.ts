export type HomeStatKey =
  | 'audios'
  | 'images'
  | 'videos'
  | 'docs'
  | 'packages'
  | 'notes'
  | 'feedEntries'
  | 'messages'
  | 'calls'
  | 'contacts'

export const HOME_STATS_COUNT_FIELDS: Record<HomeStatKey, string> = {
  audios: 'audioCount(query: $mediaQuery)',
  images: 'imageCount(query: $mediaQuery)',
  videos: 'videoCount(query: $mediaQuery)',
  docs: 'docCount(query: "")',
  packages: 'packageCount(query: "")',
  notes: 'noteCount(query: "")',
  feedEntries: 'feedEntryCount(query: "")',
  messages: 'smsCount(query: "")',
  calls: 'callCount(query: "")',
  contacts: 'contactCount(query: "")',
}

export function homeStatsGQL(countKeys: readonly HomeStatKey[] = []): string {
  const fields = countKeys.map((key) => HOME_STATS_COUNT_FIELDS[key]).join('\n    ')
  const variables = fields.includes('$mediaQuery') ? '($mediaQuery: String!)' : ''
  return `
  query homeStats${variables} {
    ${fields}
    mounts {
      id
      path
      mountPoint
      totalBytes
      freeBytes
      driveType
    }
  }
`
}
