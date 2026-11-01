import { expect, it, vi } from 'vitest'
vi.mock('@/lib/prefs/appearance', () => ({ get: (_key: string, fallback: unknown) => fallback }))
import { formatUsedTotalBytes } from '@/lib/format'

it.each([
  [83_000_000_000, 314_000_000_000, true, '83 GB / 314 GB'],
  [500_000_000_000, 3_900_000_000_000, true, '0.5 TB / 3.9 TB'],
  [65_000_000_000, 501_000_000_000, true, '65 GB / 501 GB'],
  [500, 1000, true, '0.5 kB / 1.0 kB'],
  [512, 1024, false, '0.5 KiB / 1.0 KiB'],
  [10, 20, true, '10 B / 20 B'],
  [0, 0, true, ''],
])('formats used and total bytes with the correct shared unit (%s/%s)', (used, total, si, expected) => {
  expect(formatUsedTotalBytes(used as number, total as number, si as boolean)).toBe(expected)
})
