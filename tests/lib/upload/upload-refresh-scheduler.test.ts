import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createUploadRefreshScheduler } from '@/lib/upload/refresh-scheduler'
import type { IUploadItem } from '@/stores/temp'

const doneEvent = () => ({ status: 'done' }) as IUploadItem
const uploadingEvent = () => ({ status: 'uploading' }) as IUploadItem

describe('createUploadRefreshScheduler', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('refreshes once after a single done event', () => {
    const onRefresh = vi.fn()
    const s = createUploadRefreshScheduler(onRefresh)

    s.onUploadDone(doneEvent())
    expect(onRefresh).not.toHaveBeenCalled()

    vi.advanceTimersByTime(2000)
    expect(onRefresh).toHaveBeenCalledTimes(1)
  })

  it('coalesces a burst of done events into one refresh', () => {
    const onRefresh = vi.fn()
    const s = createUploadRefreshScheduler(onRefresh)

    for (let i = 0; i < 1000; i++) {
      s.onUploadDone(doneEvent())
      vi.advanceTimersByTime(5)
    }
    vi.advanceTimersByTime(2000)

    expect(onRefresh).toHaveBeenCalledTimes(1)
  })

  it('caps the wait at maxWait under sustained completions', () => {
    const onRefresh = vi.fn()
    const s = createUploadRefreshScheduler(onRefresh, { delay: 2000, maxWait: 10000 })

    // A completion every 500ms forever: the 2s debounce never goes quiet on
    // its own, so only the maxWait cap lets a refresh through.
    for (let t = 0; t <= 20000; t += 500) {
      s.onUploadDone(doneEvent())
      vi.advanceTimersByTime(500)
    }

    expect(onRefresh.mock.calls.length).toBeGreaterThanOrEqual(2)
    expect(onRefresh.mock.calls.length).toBeLessThanOrEqual(3)
  })

  it('ignores non-done events', () => {
    const onRefresh = vi.fn()
    const s = createUploadRefreshScheduler(onRefresh)

    s.onUploadDone(uploadingEvent())
    vi.advanceTimersByTime(30000)

    expect(onRefresh).not.toHaveBeenCalled()
  })

  it('starts a new cycle after a refresh fired', () => {
    const onRefresh = vi.fn()
    const s = createUploadRefreshScheduler(onRefresh)

    s.onUploadDone(doneEvent())
    vi.advanceTimersByTime(2000)
    s.onUploadDone(doneEvent())
    vi.advanceTimersByTime(2000)

    expect(onRefresh).toHaveBeenCalledTimes(2)
  })

  it('dispose cancels the pending refresh', () => {
    const onRefresh = vi.fn()
    const s = createUploadRefreshScheduler(onRefresh)

    s.onUploadDone(doneEvent())
    s.dispose()
    vi.advanceTimersByTime(30000)

    expect(onRefresh).not.toHaveBeenCalled()
  })
})
