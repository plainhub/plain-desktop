import { describe, it, expect } from 'vitest'
import { homeStatsGQL } from '@/lib/api/query'

describe('homeStatsGQL', () => {
  it('includes only the requested count fields', () => {
    const q = homeStatsGQL(['audios', 'images', 'videos'])
    expect(q).toContain('audioCount(query: $mediaQuery)')
    expect(q).toContain('imageCount(query: $mediaQuery)')
    expect(q).toContain('videoCount(query: $mediaQuery)')
    expect(q).not.toContain('smsCount')
    expect(q).not.toContain('contactCount')
    expect(q).not.toContain('docCount')
  })

  it('always includes mounts for the storage info card', () => {
    expect(homeStatsGQL([])).toContain('mounts')
  })

  it('returns a mounts-only probe when no keys are given', () => {
    const q = homeStatsGQL([])
    expect(q).not.toContain('Count(')
    expect(q).toContain('query homeStats($mediaQuery: String!)')
  })

  it('accepts the full phone set', () => {
    const q = homeStatsGQL([
      'audios', 'images', 'videos', 'docs', 'packages',
      'notes', 'feedEntries', 'messages', 'calls', 'contacts',
    ])
    expect(q).toContain('smsCount')
    expect(q).toContain('docCount')
    expect(q).toContain('feedEntryCount')
  })
})
