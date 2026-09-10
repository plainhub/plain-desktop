import { describe, it, expect } from 'vitest'
import type { IUploadItem } from '@/stores/temp'
import { computeBatchStats, partitionBatches, keyOf } from '@/lib/upload/batch'

function item(id: string, status: string, overrides: Partial<IUploadItem> = {}): IUploadItem {
  return {
    id,
    dir: 'Documents',
    fileName: `${id}.bin`,
    file: { size: 100 } as unknown as File,
    uploadedSize: 0,
    status,
    error: '',
    createdAt: `2026-01-01T00:00:${id.padStart(2, '0')}`,
    ...overrides,
  }
}

describe('computeBatchStats', () => {
  it('keeps uploading status while other files failed, collecting the failed items', () => {
    const failed = item('2', 'error', { error: 'boom' })
    const stats = computeBatchStats([item('1', 'uploading'), failed, item('3', 'done')])
    expect(stats.status).toBe('uploading')
    expect(stats.errorCount).toBe(1)
    expect(stats.firstError).toBe('boom')
    expect(stats.failedItems).toEqual([failed])
  })

  it('labels a batch error once nothing is active anymore', () => {
    const stats = computeBatchStats([item('1', 'done'), item('2', 'error', { error: 'boom' })])
    expect(stats.status).toBe('error')
    expect(stats.errorCount).toBe(1)
  })

  it('follows uploading > saving > pending precedence', () => {
    expect(computeBatchStats([item('1', 'pending'), item('2', 'saving')]).status).toBe('saving')
    expect(computeBatchStats([item('1', 'pending'), item('2', 'uploading')]).status).toBe('uploading')
    expect(computeBatchStats([item('1', 'pending')]).status).toBe('pending')
    expect(computeBatchStats([item('1', 'pending'), item('2', 'error', { error: 'x' })]).status).toBe('pending')
  })

  it('reports paused only when every item is paused', () => {
    expect(computeBatchStats([item('1', 'paused'), item('2', 'paused')]).status).toBe('paused')
    expect(computeBatchStats([item('1', 'paused'), item('2', 'pending')]).status).toBe('pending')
  })

  it('reports done for done/canceled mixes and created otherwise', () => {
    expect(computeBatchStats([item('1', 'done'), item('2', 'canceled')]).status).toBe('done')
    expect(computeBatchStats([item('1', 'done'), item('2', 'created')]).status).toBe('created')
  })

  it('sums byte totals across the batch', () => {
    const stats = computeBatchStats([
      item('1', 'done', { file: { size: 300 } as unknown as File, uploadedSize: 300 }),
      item('2', 'uploading', { file: { size: 700 } as unknown as File, uploadedSize: 350 }),
    ])
    expect(stats.totalBytes).toBe(1000)
    expect(stats.uploadedBytes).toBe(650)
  })

  it('derives action flags from per-item states', () => {
    const stats = computeBatchStats([
      item('1', 'uploading', { pausing: true }),
      item('2', 'pending'),
      item('3', 'paused'),
      item('4', 'error', { error: 'x' }),
    ])
    expect(stats.canPause).toBe(true)
    expect(stats.canResume).toBe(true)
    expect(stats.canRetry).toBe(true)
    expect(stats.isPausing).toBe(true)
  })

  it('does not offer pause when the only pausable item is mid-pausing', () => {
    const stats = computeBatchStats([item('1', 'uploading', { pausing: true }), item('2', 'done')])
    expect(stats.canPause).toBe(false)
  })
})

describe('partitionBatches', () => {
  it('splits batches into in-progress and completed', () => {
    const { inProgress, completed } = partitionBatches([
      item('a1', 'done', { batchId: 'A' }),
      item('b1', 'done', { batchId: 'B' }),
      item('b2', 'uploading', { batchId: 'B' }),
      item('c1', 'error', { batchId: 'C' }),
    ])
    expect(inProgress.map((t) => t.batchId)).toEqual(['B'])
    expect(completed.map((t) => t.batchId)).toEqual(['A', 'C'])
  })

  it('keeps a batch in progress while any item is not completed', () => {
    const { inProgress } = partitionBatches([
      item('a1', 'done', { batchId: 'A' }),
      item('a2', 'done', { batchId: 'A' }),
      item('a3', 'pending', { batchId: 'A' }),
    ])
    expect(inProgress.map((t) => t.batchId)).toEqual(['A'])
  })

  it('sorts in-progress batches by status priority then creation time', () => {
    const { inProgress } = partitionBatches([
      item('p1', 'pending', { batchId: 'P', createdAt: '2026-01-01T00:00:01' }),
      item('u1', 'uploading', { batchId: 'U', createdAt: '2026-01-01T00:00:09' }),
      item('q1', 'pending', { batchId: 'Q', createdAt: '2026-01-01T00:00:02' }),
    ])
    expect(inProgress.map((t) => t.batchId)).toEqual(['U', 'P', 'Q'])
  })

  it('falls back to the item id when batchId is missing', () => {
    const lone = item('lone', 'done')
    expect(keyOf(lone)).toBe('lone')
    const { completed } = partitionBatches([lone])
    expect(completed.map((t) => t.batchId)).toEqual(['lone'])
  })
})
