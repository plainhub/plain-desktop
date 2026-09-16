import { describe, expect, it } from 'vitest'
import { noDataKey } from '@/lib/list'

describe('noDataKey', () => {
  it('loading wins over everything', () => {
    expect(noDataKey(true, [], 'NOTIFICATION_LISTENER', false)).toBe('loading')
  })

  it('offline device shows offline even when the permission is missing', () => {
    expect(noDataKey(false, [], 'NOTIFICATION_LISTENER', false)).toBe('offline')
    expect(noDataKey(false, ['NOTIFICATION_LISTENER'], '', false)).toBe('offline')
  })

  it('online device without the permission shows no_permission', () => {
    expect(noDataKey(false, [], 'NOTIFICATION_LISTENER')).toBe('no_permission')
  })

  it('online device with the permission but empty list shows no_data', () => {
    expect(noDataKey(false, ['NOTIFICATION_LISTENER'], 'NOTIFICATION_LISTENER')).toBe('no_data')
  })
})
