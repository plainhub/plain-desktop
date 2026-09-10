import type { IUploadItem } from '@/stores/temp'
import { compareLocale } from '@/lib/array'

export type TaskListItem = { id: string; kind: 'upload_batch'; batchId: string; uploads: IUploadItem[] }

const completedStates = new Set(['done', 'error', 'canceled'])

export const keyOf = (it: IUploadItem) => it.batchId || it.id

export function groupByBatch(uploads: IUploadItem[]): Map<string, IUploadItem[]> {
  const map = new Map<string, IUploadItem[]>()
  for (const it of uploads) {
    const k = keyOf(it)
    const list = map.get(k)
    if (list) list.push(it)
    else map.set(k, [it])
  }
  return map
}

export const batchCreatedAt = (items: IUploadItem[]) => {
  let min = ''
  for (const it of items) {
    const v = it.createdAt ?? ''
    if (!min || v < min) min = v
  }
  return min
}

export interface IBatchStats {
  status: string
  totalBytes: number
  uploadedBytes: number
  errorCount: number
  firstError: string
  canPause: boolean
  canResume: boolean
  canRetry: boolean
  isPausing: boolean
}

// Single pass over the batch. With directory uploads reaching tens of
// thousands of files, one O(n) pass per invalidation (instead of one per
// aggregate) is the difference between a usable panel and a frozen UI.
// Throughput is NOT aggregated here: per-item speeds only sample after 500ms
// of transfer, so short-lived files never report one. The batch card measures
// real throughput by diffing uploadedBytes over time instead.
export function computeBatchStats(uploads: IUploadItem[]): IBatchStats {
  let totalBytes = 0
  let uploadedBytes = 0
  let errorCount = 0
  let firstError = ''
  let count = 0
  let uploading = 0
  let saving = 0
  let pending = 0
  let paused = 0
  let doneOrCanceled = 0
  let canPause = false
  let canResume = false
  let canRetry = false
  let isPausing = false
  for (const it of uploads) {
    count++
    totalBytes += it.file?.size || 0
    uploadedBytes += it.uploadedSize || 0
    const s = it.status
    if (s === 'uploading') {
      uploading++
      if (!it.pausing) canPause = true
    } else if (s === 'saving') {
      saving++
    } else if (s === 'pending') {
      pending++
      if (!it.pausing) canPause = true
    } else if (s === 'paused') {
      paused++
      if (!it.pausing) canResume = true
    } else if (s === 'error') {
      errorCount++
      if (!firstError) firstError = it.error || ''
      canRetry = true
    } else if (s === 'done' || s === 'canceled') {
      doneOrCanceled++
    }
    if (it.pausing) isPausing = true
  }
  let status = 'created'
  if (errorCount > 0) status = 'error'
  else if (uploading > 0) status = 'uploading'
  else if (saving > 0) status = 'saving'
  else if (pending > 0) status = 'pending'
  else if (count > 0 && paused === count) status = 'paused'
  else if (count > 0 && doneOrCanceled === count) status = 'done'
  return { status, totalBytes, uploadedBytes, errorCount, firstError, canPause, canResume, canRetry, isPausing }
}

const sortKeys = new Map([
  ['uploading', 0],
  ['saving', 1],
  ['pending', 2],
  ['paused', 3],
  ['created', 4],
])

export function partitionBatches(uploads: IUploadItem[]): { inProgress: TaskListItem[]; completed: TaskListItem[] } {
  const inProgress: { entry: TaskListItem; key: number; createdAt: string }[] = []
  const completed: TaskListItem[] = []
  for (const [batchId, items] of groupByBatch(uploads)) {
    const entry: TaskListItem = { id: batchId, kind: 'upload_batch', batchId, uploads: items }
    let active = false
    for (const it of items) {
      if (!completedStates.has(it.status)) {
        active = true
        break
      }
    }
    if (active) {
      // Sort keys are computed once per batch, not inside the comparator —
      // the old code re-derived batch status on every comparison.
      inProgress.push({ entry, key: sortKeys.get(computeBatchStats(items).status) ?? 5, createdAt: batchCreatedAt(items) })
    } else {
      completed.push(entry)
    }
  }
  inProgress.sort((a, b) => (a.key !== b.key ? a.key - b.key : compareLocale(a.createdAt, b.createdAt)))
  return { inProgress: inProgress.map((it) => it.entry), completed }
}
