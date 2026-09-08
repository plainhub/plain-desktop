import { describe, it, expect } from 'vitest'
import { sortByDate } from '@/lib/message-helpers'

const msg = (id: string, date: string) => ({ id, date })

describe('sortByDate', () => {
  it('sorts ascending by date', () => {
    const items = [
      msg('c', '2026-09-06T03:00:00Z'),
      msg('a', '2026-09-06T01:00:00Z'),
      msg('b', '2026-09-06T02:00:00Z'),
    ]
    expect(sortByDate(items).map((i) => i.id)).toEqual(['a', 'b', 'c'])
  })

  it('sorts descending when requested', () => {
    const items = [
      msg('a', '2026-09-06T01:00:00Z'),
      msg('c', '2026-09-06T03:00:00Z'),
      msg('b', '2026-09-06T02:00:00Z'),
    ]
    expect(sortByDate(items, true).map((i) => i.id)).toEqual(['c', 'b', 'a'])
  })

  it('does not mutate the input array', () => {
    const items = [msg('b', '2026-09-06T02:00:00Z'), msg('a', '2026-09-06T01:00:00Z')]
    sortByDate(items)
    expect(items.map((i) => i.id)).toEqual(['b', 'a'])
  })

  it('handles empty and single-item lists', () => {
    expect(sortByDate([])).toEqual([])
    expect(sortByDate([msg('only', '2026-09-06T00:00:00Z')]).map((i) => i.id)).toEqual(['only'])
  })

  it('keeps equal timestamps in a deterministic order relative to input', () => {
    const items = [msg('x', '2026-09-06T00:00:00Z'), msg('y', '2026-09-06T00:00:00Z')]
    expect(sortByDate(items).map((i) => i.id)).toEqual(['x', 'y'])
  })
})
