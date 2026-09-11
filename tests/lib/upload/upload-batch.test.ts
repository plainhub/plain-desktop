import { describe, it, expect, beforeEach } from 'vitest'
import type { IUploadItem } from '@/stores/temp'
import { keyOf, batchStatsFromProgress, partitionBatches } from '@/lib/upload/batch'
import {
  uploadBatches,
  registerUploadItems,
  setUploadStatus,
  setUploadUploadedSize,
  resetUploadProgress,
  setUploadPausing,
  removeUploadBatch,
  hasActiveUploadBatches,
  resetUploadBatchesForTests,
} from '@/lib/upload/batch-progress'

function item(id: string, overrides: Partial<IUploadItem> = {}): IUploadItem {
  return {
    id,
    dir: 'Documents',
    fileName: `${id}.bin`,
    file: { size: 100 } as unknown as File,
    uploadedSize: 0,
    status: 'created',
    error: '',
    createdAt: `2026-01-01T00:00:${id.padStart(2, '0')}`,
    ...overrides,
  }
}

/** Drive an item through the funnel in one go, mirroring what the queue does. */
function to(status: string, ...items: IUploadItem[]) {
  for (const it of items) setUploadStatus(it, status)
}

beforeEach(() => {
  resetUploadBatchesForTests()
})

describe('batch-progress aggregates', () => {
  it('registers file/byte/status counts per batch', () => {
    const a = item('1', { batchId: 'A' })
    const b = item('2', { batchId: 'A', file: { size: 300 } as unknown as File })
    registerUploadItems([a, b])
    to('pending', a, b)
    const agg = uploadBatches.get('A')!
    expect(agg.totalFiles).toBe(2)
    expect(agg.totalBytes).toBe(400)
    expect(agg.pending).toBe(2)
  })

  it('keys single-file uploads by item id when batchId is missing', () => {
    const lone = item('lone')
    registerUploadItems([lone])
    expect(keyOf(lone)).toBe('lone')
    expect(uploadBatches.has('lone')).toBe(true)
  })

  it('moves counters across every status transition', () => {
    const it = item('1', { batchId: 'A' })
    registerUploadItems([it])
    to('pending', it)
    to('uploading', it)
    to('saving', it)
    to('done', it)
    const agg = uploadBatches.get('A')!
    expect(agg.done).toBe(1)
    expect(agg.created + agg.pending + agg.uploading + agg.saving).toBe(0)
  })

  it('tracks failed items, dropping them again on retry', () => {
    const failed = item('2', { batchId: 'A' })
    registerUploadItems([failed])
    to('uploading', failed)
    to('error', failed)
    failed.error = 'boom'
    const agg = uploadBatches.get('A')!
    expect(agg.failedItems).toEqual([failed])
    to('uploading', failed)
    expect(agg.failedItems).toEqual([])
    expect(agg.error).toBe(0)
  })

  it('aggregates byte deltas and resets them on retry', () => {
    const it = item('1', { batchId: 'A', file: { size: 700 } as unknown as File })
    registerUploadItems([it])
    setUploadUploadedSize(it, 350)
    setUploadUploadedSize(it, 700)
    expect(uploadBatches.get('A')!.uploadedBytes).toBe(700)
    to('error', it)
    resetUploadProgress(it)
    expect(uploadBatches.get('A')!.uploadedBytes).toBe(0)
    expect(it.uploadedSize).toBe(0)
  })

  it('writes fields but skips aggregates for unregistered items (chat flow)', () => {
    const chatItem = item('c1')
    to('uploading', chatItem)
    expect(chatItem.status).toBe('uploading')
    expect(uploadBatches.size).toBe(0)
    setUploadUploadedSize(chatItem, 50)
    expect(chatItem.uploadedSize).toBe(50)
  })

  it('counts pausing flags per status bucket', () => {
    const it = item('1', { batchId: 'A' })
    registerUploadItems([it])
    to('uploading', it)
    setUploadPausing(it, true)
    expect(uploadBatches.get('A')!.pausingUploading).toBe(1)
    // pause transition moves the pausing counter bucket
    to('paused', it)
    expect(uploadBatches.get('A')!.pausingUploading).toBe(0)
    expect(uploadBatches.get('A')!.pausingPaused).toBe(1)
    setUploadPausing(it, false)
    expect(uploadBatches.get('A')!.pausingPaused).toBe(0)
  })

  it('drops the batch on removal', () => {
    const it = item('1', { batchId: 'A' })
    registerUploadItems([it])
    removeUploadBatch('A')
    expect(uploadBatches.has('A')).toBe(false)
    expect(hasActiveUploadBatches()).toBe(false)
  })
})

describe('batchStatsFromProgress', () => {
  it('keeps uploading status while other files failed, collecting the failed items', () => {
    const a = item('1', { batchId: 'A' })
    const failed = item('2', { batchId: 'A' })
    const c = item('3', { batchId: 'A' })
    registerUploadItems([a, failed, c])
    to('uploading', a)
    to('error', failed)
    failed.error = 'boom'
    to('done', c)
    const stats = batchStatsFromProgress(uploadBatches.get('A')!)
    expect(stats.status).toBe('uploading')
    expect(stats.errorCount).toBe(1)
    expect(stats.firstError).toBe('boom')
    expect(stats.failedItems).toEqual([failed])
  })

  it('labels a batch error once nothing is active anymore', () => {
    const a = item('1', { batchId: 'A' })
    const b = item('2', { batchId: 'A' })
    registerUploadItems([a, b])
    to('done', a)
    to('error', b)
    const stats = batchStatsFromProgress(uploadBatches.get('A')!)
    expect(stats.status).toBe('error')
    expect(stats.errorCount).toBe(1)
  })

  it('follows uploading > saving > pending precedence', () => {
    const p = item('1', { batchId: 'P' })
    registerUploadItems([p])
    to('pending', p)
    expect(batchStatsFromProgress(uploadBatches.get('P')!).status).toBe('pending')

    const s = item('1', { batchId: 'S' })
    registerUploadItems([s])
    to('saving', s)
    const mixed = uploadBatches.get('S')!
    // pending + saving
    const p2 = item('2', { batchId: 'S' })
    registerUploadItems([p2])
    to('pending', p2)
    expect(batchStatsFromProgress(mixed).status).toBe('saving')
  })

  it('reports paused only when every item is paused', () => {
    const a = item('1', { batchId: 'A' })
    const b = item('2', { batchId: 'A' })
    registerUploadItems([a, b])
    to('paused', a)
    to('paused', b)
    expect(batchStatsFromProgress(uploadBatches.get('A')!).status).toBe('paused')
    to('pending', b)
    expect(batchStatsFromProgress(uploadBatches.get('A')!).status).toBe('pending')
  })

  it('reports done for done/canceled mixes and pending otherwise', () => {
    const a = item('1', { batchId: 'A' })
    const b = item('2', { batchId: 'A' })
    registerUploadItems([a, b])
    to('done', a)
    to('canceled', b)
    expect(batchStatsFromProgress(uploadBatches.get('A')!).status).toBe('done')
  })

  it('sums byte totals across the batch', () => {
    const a = item('1', { batchId: 'A', file: { size: 300 } as unknown as File })
    const b = item('2', { batchId: 'A', file: { size: 700 } as unknown as File })
    registerUploadItems([a, b])
    setUploadUploadedSize(a, 300)
    setUploadUploadedSize(b, 350)
    const stats = batchStatsFromProgress(uploadBatches.get('A')!)
    expect(stats.totalBytes).toBe(1000)
    expect(stats.uploadedBytes).toBe(650)
  })

  it('derives action flags from per-item states', () => {
    const a = item('1', { batchId: 'A' })
    const b = item('2', { batchId: 'A' })
    const c = item('3', { batchId: 'A' })
    const d = item('4', { batchId: 'A' })
    registerUploadItems([a, b, c, d])
    to('uploading', a)
    to('pending', b)
    to('paused', c)
    to('error', d)
    const stats = batchStatsFromProgress(uploadBatches.get('A')!)
    expect(stats.canPause).toBe(true)
    expect(stats.canResume).toBe(true)
    expect(stats.canRetry).toBe(true)
    expect(stats.isPausing).toBe(false)
  })

  it('does not offer pause when the only pausable item is mid-pausing', () => {
    const a = item('1', { batchId: 'A' })
    const b = item('2', { batchId: 'A' })
    registerUploadItems([a, b])
    to('uploading', a)
    to('done', b)
    setUploadPausing(a, true)
    const stats = batchStatsFromProgress(uploadBatches.get('A')!)
    expect(stats.canPause).toBe(false)
    expect(stats.isPausing).toBe(true)
  })
})

describe('partitionBatches', () => {
  function batch(batchId: string, statuses: string[], createdAt = `2026-01-01T00:00:0${batchId.length % 10}`) {
    const items = statuses.map((s, i) => item(`${batchId}${i}`, { batchId, createdAt: `${createdAt}${i}` }))
    registerUploadItems(items)
    items.forEach((it, i) => to(statuses[i], it))
    return items
  }

  it('splits batches into in-progress and completed', () => {
    batch('A', ['done'])
    batch('B', ['done', 'uploading'])
    batch('C', ['error'])
    const { inProgress, completed } = partitionBatches(uploadBatches)
    expect(inProgress.map((t) => t.batchId)).toEqual(['B'])
    expect(completed.map((t) => t.batchId)).toEqual(['A', 'C'])
  })

  it('keeps a batch in progress while any item is not completed', () => {
    batch('A', ['done', 'done', 'pending'])
    const { inProgress } = partitionBatches(uploadBatches)
    expect(inProgress.map((t) => t.batchId)).toEqual(['A'])
  })

  it('sorts in-progress batches by status priority then creation time', () => {
    const p = item('p1', { batchId: 'P', createdAt: '2026-01-01T00:00:01' })
    const u = item('u1', { batchId: 'U', createdAt: '2026-01-01T00:00:09' })
    const q = item('q1', { batchId: 'Q', createdAt: '2026-01-01T00:00:02' })
    registerUploadItems([p, u, q])
    to('pending', p)
    to('uploading', u)
    to('pending', q)
    const { inProgress } = partitionBatches(uploadBatches)
    expect(inProgress.map((t) => t.batchId)).toEqual(['U', 'P', 'Q'])
  })
})
