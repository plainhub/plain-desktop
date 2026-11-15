import { expect, it } from 'vitest'
import { formatHistoryLabel } from '@/components/header-search/formatHistoryLabel'
import { fileSizeOptions } from '@/components/header-search/options'
import { displayValue } from '@/lib/token-search-format'

const t = ((key: string) => key === 'file_size' ? 'File size' : key) as Parameters<typeof displayValue>[0]

it('shows the same file size in the selected chip and search history', () => {
  for (const { value, label } of fileSizeOptions(t)) {
    expect(displayValue(t, {}, 'file_size', value)).toBe(label)
    expect(formatHistoryLabel({
      decoded: `file_size:${value}`, group: 'files', t, mediaBuckets: [], mediaTags: [], feeds: [],
    })).toBe(`File size: ${label}`)
  }
})

it('preserves the operator, exact byte threshold, and manually entered units', () => {
  expect(displayValue(t, {}, 'file_size', '>= 1048576')).toBe('>= 1MB')
  expect(displayValue(t, {}, 'file_size', '>1048577')).toBe('> 1048577')
  expect(displayValue(t, {}, 'file_size', '<1MB')).toBe('< 1MB')
})
