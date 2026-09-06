import { describe, expect, it } from 'vitest'
import { bootstrapErrorDetail } from '@/bootstrap/bootstrap-error-detail'

describe('bootstrapErrorDetail', () => {
  it('formats Error instances without their stack', () => {
    expect(bootstrapErrorDetail(new TypeError('overlay failed'))).toBe(
      'TypeError: overlay failed',
    )
  })

  it('handles undefined values that JSON.stringify cannot serialize', () => {
    expect(bootstrapErrorDetail(undefined)).toBe('undefined')
  })

  it('falls back for cyclic values', () => {
    const value: { self?: unknown } = {}
    value.self = value
    expect(bootstrapErrorDetail(value)).toBe('[object Object]')
  })

  it('bounds client detail', () => {
    expect(bootstrapErrorDetail('x'.repeat(2048))).toHaveLength(1024)
  })
})
