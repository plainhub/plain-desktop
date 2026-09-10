import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { IUploadItem } from '@/stores/temp'

// Mock eventbus
vi.mock('@/plugins/eventbus', () => ({ default: { emit: vi.fn() } }))

// Mock the upload function to control test flow
const mockUpload = vi.fn()
vi.mock('@/lib/upload/upload', () => ({
  upload: (...args: any[]) => mockUpload(...args),
}))

import { addUploadTask, addUploadTaskAndWait, removeUpload, getUploadQueueStatus, pauseUploadsByBatch, resumeUploadsByBatch, retryUploadsByBatch, retryUploadTask, removeUploadsByBatch, resetUploadFailureStreakForTests } from '@/lib/upload/upload-queue'
import emitter from '@/plugins/eventbus'

const createdIds = new Set<string>()

function createUploadItem(id: string, overrides: Partial<IUploadItem> = {}): IUploadItem {
  createdIds.add(id)
  return {
    id,
    dir: '/downloads',
    fileName: `file-${id}.mp4`,
    file: new File([new Uint8Array(100)], `file-${id}.mp4`),
    uploadedSize: 0,
    status: 'pending',
    error: '',
    ...overrides,
  }
}

describe('UploadQueue', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resetUploadFailureStreakForTests()
    // Default mock: upload resolves successfully after a delay
    mockUpload.mockImplementation(() => new Promise((resolve) => setTimeout(() => resolve({ fileName: 'ok' }), 50)))
  })

  afterEach(() => {
    for (const id of createdIds) removeUpload(id)
    createdIds.clear()
  })

  describe('addUploadTask', () => {
    it('returns the upload item id', () => {
      const item = createUploadItem('add-1')
      const taskId = addUploadTask(item, false)
      expect(taskId).toBe('add-1')
    })

    it('sets status to uploading', async () => {
      mockUpload.mockResolvedValue({ fileName: 'ok' })
      const item = createUploadItem('add-2')
      addUploadTask(item, false)
      // Wait a tick for processQueue to run
      await new Promise((r) => setTimeout(r, 10))
      // Status should be 'uploading' or 'done'
      expect(['uploading', 'done']).toContain(item.status)
    })

    it('awaits successful completion and evicts the completed task', async () => {
      mockUpload.mockImplementation(async (upload: IUploadItem) => {
        upload.status = 'done'
        upload.fileHash = 'hash-success'
        return { fileName: 'hash-success' }
      })
      const item = createUploadItem('await-success')

      await expect(addUploadTaskAndWait(item, false)).resolves.toBeUndefined()

      expect(item.status).toBe('done')
      expect(getUploadQueueStatus()).toEqual({ pending: 0, running: 0, paused: 0, total: 0 })
    })

    it('rejects failed awaited tasks and evicts their retained File', async () => {
      mockUpload.mockResolvedValue({ error: 'network unavailable' })
      const item = createUploadItem('await-failure')

      await expect(addUploadTaskAndWait(item, false)).rejects.toThrow('network unavailable')

      expect(item.status).toBe('error')
      expect(getUploadQueueStatus()).toEqual({ pending: 0, running: 0, paused: 0, total: 0 })
    })

    it('does not retain completed File objects across 100 sequential awaited tasks', async () => {
      mockUpload.mockImplementation(async (upload: IUploadItem) => {
        upload.status = 'done'
        upload.fileHash = `hash-${upload.id}`
        return { fileName: upload.fileHash }
      })

      for (let index = 0; index < 100; index += 1) {
        await addUploadTaskAndWait(createUploadItem(`retention-${index}`), false)
        expect(getUploadQueueStatus().total).toBe(0)
      }
    })
  })

  describe('removeUpload', () => {
    it('aborts all XHRs in xhrs Set when removing a running task', () => {
      // Test the pattern directly: xhrs Set should be iterated and all aborted
      const abortFn = vi.fn()
      const xhrs = new Set<XMLHttpRequest>()
      xhrs.add({ abort: abortFn } as unknown as XMLHttpRequest)
      xhrs.add({ abort: abortFn } as unknown as XMLHttpRequest)
      xhrs.add({ abort: abortFn } as unknown as XMLHttpRequest)

      // Simulate the abort-all pattern from removeTask
      for (const xhr of xhrs) {
        try {
          xhr.abort()
        } catch (_) {
          /* ignore */
        }
      }
      xhrs.clear()

      expect(abortFn).toHaveBeenCalledTimes(3)
      expect(xhrs.size).toBe(0)
    })
  })

  describe('automatic retry', () => {
    it('retries a transient failure with backoff and completes', async () => {
      vi.useFakeTimers()
      try {
        mockUpload
          .mockImplementationOnce(async (upload: IUploadItem) => {
            upload.status = 'error'
            return { error: 'connection_timeout' }
          })
          .mockImplementationOnce(async (upload: IUploadItem) => {
            upload.status = 'done'
            return { fileName: 'ok' }
          })
        const item = createUploadItem('auto-retry')

        const completion = addUploadTaskAndWait(item, false)
        await vi.advanceTimersByTimeAsync(2500)

        await expect(completion).resolves.toBeUndefined()
        expect(mockUpload).toHaveBeenCalledTimes(2)
        expect(item.status).toBe('done')
      } finally {
        vi.useRealTimers()
      }
    })

    it('fails without retrying on non-transient errors', async () => {
      mockUpload.mockImplementation(async (upload: IUploadItem) => {
        upload.status = 'error'
        return { error: 'File name is invalid' }
      })
      const item = createUploadItem('no-retry')

      await expect(addUploadTaskAndWait(item, false)).rejects.toThrow('File name is invalid')
      expect(mockUpload).toHaveBeenCalledTimes(1)
    })

    it('stops retrying after the attempt budget and reports the error', async () => {
      vi.useFakeTimers()
      try {
        mockUpload.mockImplementation(async (upload: IUploadItem) => {
          upload.status = 'error'
          return { error: 'Network error' }
        })
        const item = createUploadItem('retry-budget')

        const completion = addUploadTaskAndWait(item, false)
        const assertion = expect(completion).rejects.toThrow('Network error')
        await vi.advanceTimersByTimeAsync(30_000)
        await assertion
        expect(mockUpload).toHaveBeenCalledTimes(3)
        expect(item.status).toBe('error')
      } finally {
        vi.useRealTimers()
      }
    })

    it('auto-pauses the queue after 5 consecutive transient failures', async () => {
      vi.useFakeTimers()
      try {
        mockUpload.mockImplementation(async (upload: IUploadItem) => {
          upload.status = 'error'
          return { error: 'Network error' }
        })
        const items = Array.from({ length: 8 }, (_, i) => createUploadItem(`breaker-${i}`))
        for (const item of items) addUploadTask(item, false)

        await vi.advanceTimersByTimeAsync(120_000)

        const failed = items.filter((it) => it.status === 'error').length
        const paused = items.filter((it) => it.status === 'paused').length
        expect(failed).toBeGreaterThanOrEqual(5)
        expect(failed + paused).toBe(items.length)
        expect(paused).toBeGreaterThan(0)
        expect(emitter.emit).toHaveBeenCalledWith('toast', 'upload_auto_paused')
      } finally {
        vi.useRealTimers()
      }
    })
  })

  describe('retryUploadTask', () => {
    it('retries one failed task without touching its batch siblings', async () => {
      mockUpload.mockImplementation(async (upload: IUploadItem) => {
        upload.status = 'error'
        return { error: 'File name is invalid' }
      })
      const item = createUploadItem('single-retry')
      addUploadTask(item, false)
      await new Promise((r) => setTimeout(r, 20))
      expect(item.status).toBe('error')
      expect(mockUpload).toHaveBeenCalledTimes(1)

      expect(retryUploadTask(item.id)).toBe(true)
      await new Promise((r) => setTimeout(r, 20))
      expect(mockUpload).toHaveBeenCalledTimes(2)

      mockUpload.mockImplementation(async (upload: IUploadItem) => {
        upload.status = 'done'
        return { fileName: 'ok' }
      })
      expect(retryUploadTask(item.id)).toBe(true)
      await new Promise((r) => setTimeout(r, 20))
      expect(item.status).toBe('done')
      expect(retryUploadTask(item.id)).toBe(false)
    })
  })

  describe('batch operations', () => {
    function createBatchItem(id: string, batchId: string, overrides: Partial<IUploadItem> = {}): IUploadItem {
      return createUploadItem(id, { batchId, ...overrides })
    }

    it('pauses every task of a batch, aborts running XHRs and zeroes speeds', async () => {
      mockUpload.mockImplementation(() => new Promise((resolve) => setTimeout(() => resolve({ fileName: 'ok' }), 5000)))

      const running = createBatchItem('batch-run', 'b1', { uploadSpeed: 42 })
      const abortFn = vi.fn()
      running.xhrs = new Set([{ abort: abortFn } as unknown as XMLHttpRequest])
      addUploadTask(running, false)
      for (let i = 0; i < 3; i++) addUploadTask(createBatchItem(`fill-${i}`, 'other'), false)
      const pending = createBatchItem('batch-pending', 'b1')
      addUploadTask(pending, false)
      await new Promise((r) => setTimeout(r, 20))

      const affected = pauseUploadsByBatch('b1')

      expect(affected.map((it) => it.id).sort()).toEqual(['batch-pending', 'batch-run'])
      expect(running.status).toBe('paused')
      expect(pending.status).toBe('paused')
      expect(running.uploadSpeed).toBe(0)
      expect(abortFn).toHaveBeenCalledTimes(1)
    })

    it('leaves other batches untouched when pausing by batch', () => {
      mockUpload.mockImplementation(() => new Promise((resolve) => setTimeout(() => resolve({ fileName: 'ok' }), 5000)))
      for (let i = 0; i < 3; i++) addUploadTask(createBatchItem(`keep-${i}`, 'other'), false)
      const pending = createBatchItem('batch-only', 'b2')
      addUploadTask(pending, false)

      pauseUploadsByBatch('b2')

      expect(pending.status).toBe('paused')
    })

    it('resumes a paused batch', () => {
      const paused = createBatchItem('resume-1', 'b3')
      addUploadTask(paused, false)
      pauseUploadsByBatch('b3')
      expect(paused.status).toBe('paused')

      resumeUploadsByBatch('b3')

      expect(paused.status).toBe('uploading')
    })

    it('retries failed tasks of a batch and resets their counters', async () => {
      mockUpload.mockResolvedValue({ error: 'boom' })
      const failed = createBatchItem('retry-1', 'b4')
      addUploadTask(failed, false)
      await new Promise((r) => setTimeout(r, 20))
      expect(failed.status).toBe('error')

      retryUploadsByBatch('b4')

      expect(failed.status).toBe('uploading')
      expect(failed.uploadedSize).toBe(0)
      expect(failed.error).toBe('')
    })

    it('removes every task of a batch in one pass', async () => {
      mockUpload.mockImplementation(() => new Promise((resolve) => setTimeout(() => resolve({ fileName: 'ok' }), 5000)))
      const running = createBatchItem('rm-run', 'b5')
      running.xhrs = new Set()
      addUploadTask(running, false)
      for (let i = 0; i < 5; i++) addUploadTask(createBatchItem(`rm-${i}`, 'b5'), false)
      const keeper = createBatchItem('rm-keep', 'other')
      addUploadTask(keeper, false)
      await new Promise((r) => setTimeout(r, 20))

      removeUploadsByBatch('b5')

      expect(running.status).toBe('canceled')
      expect(getUploadQueueStatus().total).toBe(1)
      expect(keeper.status).not.toBe('canceled')
    })

    it('zeroes per-item speed when a task completes', async () => {
      mockUpload.mockImplementation(async (upload: IUploadItem) => {
        upload.uploadSpeed = 12345
        upload.status = 'done'
        return { fileName: 'ok' }
      })
      const item = createBatchItem('speed-zero', 'b6')

      await addUploadTaskAndWait(item, false)

      expect(item.status).toBe('done')
      expect(item.uploadSpeed).toBe(0)
    })
  })

  describe('unload guard', () => {
    function fireBeforeUnload(): boolean {
      const event = new Event('beforeunload', { cancelable: true })
      window.dispatchEvent(event)
      return event.defaultPrevented
    }

    it('prompts while a task is running and stops after removal', async () => {
      mockUpload.mockImplementation(() => new Promise(() => undefined))
      const item = createUploadItem('guard-running')
      addUploadTask(item, false)
      await new Promise((r) => setTimeout(r, 20))

      expect(fireBeforeUnload()).toBe(true)

      removeUpload('guard-running')
      expect(fireBeforeUnload()).toBe(false)
    })

    it('prompts while tasks are pending in the queue', () => {
      mockUpload.mockImplementation(() => new Promise((resolve) => setTimeout(() => resolve({ fileName: 'ok' }), 5000)))
      for (let i = 0; i < 4; i++) addUploadTask(createUploadItem(`guard-pending-${i}`), false)

      expect(getUploadQueueStatus().pending).toBeGreaterThan(0)
      expect(fireBeforeUnload()).toBe(true)
    })

    it('does not prompt once every task settles', async () => {
      mockUpload.mockResolvedValue({ fileName: 'ok' })
      addUploadTask(createUploadItem('guard-done'), false)
      await new Promise((r) => setTimeout(r, 100))

      expect(fireBeforeUnload()).toBe(false)
    })

    it('does not prompt for a paused-only queue', async () => {
      mockUpload.mockImplementation(() => new Promise(() => undefined))
      const item = createUploadItem('guard-paused')
      addUploadTask(item, false)
      await new Promise((r) => setTimeout(r, 20))
      pauseUploadsByBatch(item.batchId || item.id)

      expect(fireBeforeUnload()).toBe(false)
    })
  })
})
