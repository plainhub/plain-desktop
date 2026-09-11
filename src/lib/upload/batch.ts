import type { IUploadItem } from '@/stores/temp'
import { compareLocale } from '@/lib/array'
import type { BatchProgress } from './batch-progress'

export { keyOf } from './batch-progress'

export const batchCreatedAt = (items: IUploadItem[]) => {
  let min = ''
  for (const it of items) {
    const v = it.createdAt ?? ''
    if (!min || v < min) min = v
  }
  return min
}

export type TaskListItem = { id: string; kind: 'upload_batch'; batchId: string }

export interface IBatchStats {
  status: string
  totalBytes: number
  uploadedBytes: number
  errorCount: number
  firstError: string
  failedItems: IUploadItem[]
  canPause: boolean
  canResume: boolean
  canRetry: boolean
  isPausing: boolean
}

// O(1) derivation from the incremental aggregate maintained by
// batch-progress.ts. Same status precedence as the pre-aggregate
// computeBatchStats: active states outrank 'error' so a huge batch keeps
// uploading (and keeps its progress bar) while a few files failed.
export function batchStatsFromProgress(agg: BatchProgress): IBatchStats {
  let status = 'created'
  if (agg.uploading > 0) status = 'uploading'
  else if (agg.saving > 0) status = 'saving'
  else if (agg.pending > 0) status = 'pending'
  else if (agg.error > 0) status = 'error'
  else if (agg.totalFiles > 0 && agg.paused === agg.totalFiles) status = 'paused'
  else if (agg.totalFiles > 0 && agg.done + agg.canceled === agg.totalFiles) status = 'done'
  return {
    status,
    totalBytes: agg.totalBytes,
    uploadedBytes: agg.uploadedBytes,
    errorCount: agg.error,
    firstError: agg.failedItems[0]?.error || '',
    failedItems: agg.failedItems,
    canPause: agg.uploading + agg.pending - agg.pausingUploading - agg.pausingPending > 0,
    canResume: agg.paused - agg.pausingPaused > 0,
    canRetry: agg.error > 0,
    isPausing:
      agg.pausingCreated + agg.pausingPending + agg.pausingUploading + agg.pausingSaving + agg.pausingPaused > 0,
  }
}

const sortKeys = new Map([
  ['uploading', 0],
  ['saving', 1],
  ['pending', 2],
  ['paused', 3],
  ['created', 4],
])

export function partitionBatches(batches: Map<string, BatchProgress>): { inProgress: TaskListItem[]; completed: TaskListItem[] } {
  const inProgress: { entry: TaskListItem; key: number; createdAt: string }[] = []
  const completed: TaskListItem[] = []
  for (const [batchId, agg] of batches) {
    const entry: TaskListItem = { id: batchId, kind: 'upload_batch', batchId }
    const active = agg.created + agg.pending + agg.uploading + agg.saving + agg.paused > 0
    if (active) {
      inProgress.push({
        entry,
        key: sortKeys.get(batchStatsFromProgress(agg).status) ?? 5,
        createdAt: agg.createdAt,
      })
    } else {
      completed.push(entry)
    }
  }
  inProgress.sort((a, b) => (a.key !== b.key ? a.key - b.key : compareLocale(a.createdAt, b.createdAt)))
  return { inProgress: inProgress.map((it) => it.entry), completed }
}
