import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { IUploadItem } from '@/stores/temp'
const io = vi.hoisted(() => ({ upload: vi.fn(), discard: vi.fn(), emit: vi.fn() }))
vi.mock('@/lib/upload/upload', () => ({ upload: io.upload, discardUploadChunks: io.discard }))
vi.mock('@/lib/upload/batch-progress', () => ({ setUploadStatus: (item: IUploadItem, status: string) => { item.status = status }, resetUploadProgress: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: io.emit } }))
vi.mock('@/lib/api/gql-client', () => ({ GqlError: class extends Error {} }))
import { GqlError } from '@/lib/api/gql-client'
let queue: typeof import('@/lib/upload/upload-queue')
function item(id: string, fileId = 'a'.repeat(32), batchId = id): IUploadItem {
  return { id, batchId, fileId, fileName: 'sample.bin', file: new File(['payload'], 'sample.bin'), dir: '/owned', status: 'created', uploadedSize: 0, error: '' }
}
beforeEach(async () => {
  vi.resetModules()
  vi.stubGlobal('window', new EventTarget())
  io.upload.mockReset().mockImplementation(() => new Promise(() => {}))
  io.discard.mockReset().mockResolvedValue(undefined)
  io.emit.mockReset()
  queue = await import('@/lib/upload/upload-queue')
})
afterEach(() => vi.unstubAllGlobals())
it('aborts each removed request once and schedules cleanup once for a shared file ID', async () => {
  const a = item('first', undefined, 'batch'); const b = item('second', undefined, 'batch')
  const abortA = vi.fn(); const abortB = vi.fn()
  a.xhr = { abort: abortA } as unknown as XMLHttpRequest
  b.xhr = { abort: abortB } as unknown as XMLHttpRequest
  queue.addUploadTask(a, false); queue.addUploadTask(b, false)
  queue.removeUploadsByBatch('batch')
  expect(abortA).toHaveBeenCalledTimes(1)
  expect(abortB).toHaveBeenCalledTimes(1)
  expect(a.status).toBe('canceled'); expect(b.status).toBe('canceled')
  expect(io.discard).toHaveBeenCalledTimes(1)
  expect(io.discard.mock.calls[0]![0]).toBe(a.fileId)
  expect(io.discard.mock.calls[0]![1]()).toBe(false)
})
it('retains chunks referenced by a different paused batch', () => {
  const a = item('removed'); const b = item('retained')
  queue.addUploadTask(a, false); queue.addUploadTask(b, false)
  queue.pauseUploadsByBatch('retained')
  expect(io.discard).not.toHaveBeenCalled()
  expect(queue.removeUpload(a.id)).toBe(true)
  expect(io.discard).toHaveBeenCalledTimes(1)
  expect(io.discard.mock.calls[0]![1]()).toBe(true)
  expect(b.status).toBe('paused')
})
it('does not discard chunks for a task that never acquired a file ID', () => {
  const a = item('not-started'); delete a.fileId
  queue.addUploadTask(a, false)
  queue.removeUpload(a.id)
  expect(io.discard).not.toHaveBeenCalled()
  expect(queue.removeUpload('missing')).toBe(false)
})
it.each([true, false])('reports a cleanup failure using the existing toast error contract: %s', async graphql => {
  io.discard.mockRejectedValueOnce(graphql ? new GqlError('Permission denied') : new Error('connection lost'))
  const a = item('removed'); queue.addUploadTask(a, false); queue.removeUpload(a.id)
  await vi.waitFor(() => expect(io.emit).toHaveBeenCalledWith('toast', graphql ? 'Permission denied' : 'network_error'))
})
it.each([true, false])('discards an evicted failed transaction but preserves retryable tasks: %s', async transactional => {
  io.upload.mockResolvedValueOnce({ error: 'Permission denied' })
  const a = item('failed')
  if (transactional) await expect(queue.addUploadTaskAndWait(a, false)).rejects.toThrow('Permission denied')
  else { queue.addUploadTask(a, false); await vi.waitFor(() => expect(a.status).toBe('error')) }
  expect(io.discard).toHaveBeenCalledTimes(transactional ? 1 : 0)
})
