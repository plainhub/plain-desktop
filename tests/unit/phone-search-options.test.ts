import { expect, it } from 'vitest'
import { valueOptionsForGroup } from '@/components/header-search/options'

it.each([
  ['messages', ['INBOX', 'SENT', 'DRAFT']],
  ['calls', ['INCOMING', 'OUTGOING', 'MISSED']],
] as const)('uses enum names for %s type searches', (group, types) => {
  const options = valueOptionsForGroup({ group, t: (key) => key, feeds: [], mediaTags: [], mediaBuckets: [], history: [] })
  expect(options.type.map((item) => item.value)).toEqual(types)
})
