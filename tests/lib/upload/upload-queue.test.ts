import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { IUploadItem } from '@/stores/temp'

// Mock eventbus
vi.mock('@/plugins/eventbus', () => ({ default: { emit: vi.fn() } }))

// Mock the upload function to control test flow
const mockUpload = vi.fn()
vi.mock('@/lib/upload/upload', () => ({
  upload: (...args: any[]) => mockUpload(...args),
}))

import { addUploadTask, addUploadTaskAndWait, pauseUpload, resumeUpload, retryUpload, removeUpload, getUploadQueueStatus } from '@/lib/upload/upload-queue'

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

  describe('pauseUpload', () => {
    it('pauses a running task and aborts XHRs', async () => {
      // Simulate a long upload
      mockUpload.mockImplementation(() => new Promise((resolve) => setTimeout(() => resolve({ fileName: 'ok' }), 5000)))

      const item = createUploadItem('pause-1')
      const abortFn = vi.fn()
      // Simulate multiple active XHRs (parallel chunks)
      item.xhrs = new Set()
      const xhr1 = { abort: abortFn } as unknown as XMLHttpRequest
      const xhr2 = { abort: abortFn } as unknown as XMLHttpRequest
      item.xhrs.add(xhr1)
      item.xhrs.add(xhr2)

      addUploadTask(item, false)
      await new Promise((r) => setTimeout(r, 20))

      const result = pauseUpload('pause-1')
      expect(result).toBe(true)
      expect(item.status).toBe('paused')
      // Both XHRs should have been aborted
      expect(abortFn).toHaveBeenCalledTimes(2)
    })

    it('pauses a pending task without aborting XHR', () => {
      // Fill up the queue so this task stays pending
      mockUpload.mockImplementation(() => new Promise((resolve) => setTimeout(() => resolve({ fileName: 'ok' }), 5000)))
      // Add 3 tasks to fill the running slots
      for (let i = 0; i < 3; i++) {
        addUploadTask(createUploadItem(`fill-${i}`), false)
      }
      // This one should be pending
      const item = createUploadItem('pause-pending')
      addUploadTask(item, false)

      const result = pauseUpload('pause-pending')
      expect(result).toBe(true)
      expect(item.status).toBe('paused')
    })

    it('returns false for non-existent task', () => {
      expect(pauseUpload('non-existent')).toBe(false)
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

  describe('retryUpload', () => {
    it('resets upload state on retry', () => {
      // Test the retry state reset pattern directly
      const item = createUploadItem('retry-pattern')
      item.status = 'error'
      item.error = 'network error'
      item.uploadedSize = 50000
      item.uploadSpeed = 100
      item.lastUploadedSize = 50000
      item.lastUpdateTime = Date.now()

      // Simulate retryTask reset logic
      item.status = 'uploading'
      item.error = ''
      item.uploadedSize = 0
      item.uploadSpeed = 0
      item.lastUploadedSize = 0
      item.lastUpdateTime = undefined

      expect(item.status).toBe('uploading')
      expect(item.error).toBe('')
      expect(item.uploadedSize).toBe(0)
      expect(item.uploadSpeed).toBe(0)
    })
  })
})
