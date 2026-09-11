import { reactive } from 'vue'
import type { IUploadItem } from '@/stores/temp'

/**
 * Incremental per-batch aggregates for the uploads UI.
 *
 * With directory uploads reaching tens of thousands of files, any UI computed
 * that rescans every upload item re-traverses a deep-reactive array on each
 * event-loop flush (progress ticks + completions) — measured at 39ms (JSC) /
 * 90ms (V8) per flush at 18k items, saturating the main thread at high
 * completion rates. Instead, the upload pipeline reports every status/progress
 * change through the mutators below, which maintain O(1) counters on a small
 * reactive per-batch record. UI computeds read only these records.
 *
 * Items that never enter `tempStore.uploads` (chat transactional uploads) are
 * not registered; mutators still write their fields and simply skip the
 * aggregate bookkeeping.
 */

export type UploadStatusKey =
  | 'created'
  | 'pending'
  | 'uploading'
  | 'saving'
  | 'paused'
  | 'error'
  | 'done'
  | 'canceled'

const STATUS_KEYS: readonly UploadStatusKey[] = [
  'created',
  'pending',
  'uploading',
  'saving',
  'paused',
  'error',
  'done',
  'canceled',
]

export interface BatchProgress {
  /** ISO timestamp of the first item — stable ordering key for the batch list. */
  createdAt: string
  totalFiles: number
  totalBytes: number
  uploadedBytes: number
  created: number
  pending: number
  uploading: number
  saving: number
  paused: number
  error: number
  done: number
  canceled: number
  /** Items with `pausing: true`, counted per status bucket so action flags can
   * exclude items whose only pausable instance is mid-transition. */
  pausingCreated: number
  pausingPending: number
  pausingUploading: number
  pausingSaving: number
  pausingPaused: number
  failedItems: IUploadItem[]
}

export const uploadBatches = reactive(new Map<string, BatchProgress>())

const registered = new WeakSet<object>()

export const keyOf = (item: IUploadItem): string => item.batchId || item.id

function statusKey(status: string): UploadStatusKey {
  return (STATUS_KEYS as readonly string[]).includes(status) ? (status as UploadStatusKey) : 'created'
}

function pausingKeyOf(status: string): keyof BatchProgress | undefined {
  switch (statusKey(status)) {
    case 'created':
      return 'pausingCreated'
    case 'pending':
      return 'pausingPending'
    case 'uploading':
      return 'pausingUploading'
    case 'saving':
      return 'pausingSaving'
    case 'paused':
      return 'pausingPaused'
    default:
      return undefined
  }
}

function zeroProgress(item: IUploadItem): BatchProgress {
  return {
    createdAt: item.createdAt ?? '',
    totalFiles: 0,
    totalBytes: 0,
    uploadedBytes: 0,
    created: 0,
    pending: 0,
    uploading: 0,
    saving: 0,
    paused: 0,
    error: 0,
    done: 0,
    canceled: 0,
    pausingCreated: 0,
    pausingPending: 0,
    pausingUploading: 0,
    pausingSaving: 0,
    pausingPaused: 0,
    failedItems: [],
  }
}

/** Track newly created items. Call once, right after items enter
 * `tempStore.uploads` (their status is still `created`). */
export function registerUploadItems(items: IUploadItem[]): void {
  for (const item of items) {
    if (registered.has(item)) continue
    registered.add(item)
    const key = keyOf(item)
    let agg = uploadBatches.get(key)
    if (!agg) {
      agg = zeroProgress(item)
      uploadBatches.set(key, agg)
    }
    agg.totalFiles++
    agg.totalBytes += item.file?.size || 0
    agg.uploadedBytes += item.uploadedSize || 0
    agg[statusKey(item.status)]++
  }
}

function progressOf(item: IUploadItem): BatchProgress | undefined {
  return registered.has(item) ? uploadBatches.get(keyOf(item)) : undefined
}

export function setUploadStatus(item: IUploadItem, next: string): void {
  const prev = item.status
  if (prev === next) return
  const agg = progressOf(item)
  if (agg) {
    agg[statusKey(prev)]--
    agg[statusKey(next)]++
    const pausingFrom = pausingKeyOf(prev)
    const pausingTo = pausingKeyOf(next)
    // Terminal statuses carry no pausing bucket — drop the stale count there;
    // the pausing timeout can't decrement again for a statusless bucket.
    if (item.pausing && pausingFrom) {
      ;(agg as any)[pausingFrom]--
      if (pausingTo) (agg as any)[pausingTo]++
    }
    if (next === 'error') {
      agg.failedItems.push(item)
    } else if (prev === 'error') {
      const index = agg.failedItems.indexOf(item)
      if (index >= 0) agg.failedItems.splice(index, 1)
    }
  }
  item.status = next
}

export function setUploadUploadedSize(item: IUploadItem, bytes: number): void {
  const agg = progressOf(item)
  if (agg) agg.uploadedBytes += bytes - (item.uploadedSize || 0)
  item.uploadedSize = bytes
}

/** Retry reset: zero the byte counters (failedItems was already drained by the
 * status transition). */
export function resetUploadProgress(item: IUploadItem): void {
  const agg = progressOf(item)
  if (agg) agg.uploadedBytes -= item.uploadedSize || 0
  item.uploadedSize = 0
  item.uploadSpeed = 0
  item.lastUploadedSize = 0
  item.lastUpdateTime = undefined
}

export function setUploadPausing(item: IUploadItem, value: boolean): void {
  if (!!item.pausing === value) return
  const agg = progressOf(item)
  if (agg) {
    const key = pausingKeyOf(item.status)
    if (key) (agg as any)[key] += value ? 1 : -1
  }
  item.pausing = value
}

/** Drop a batch's aggregate when its items leave `tempStore.uploads`. */
export function removeUploadBatch(batchId: string): void {
  uploadBatches.delete(batchId)
}

export function hasActiveUploadBatches(): boolean {
  for (const agg of uploadBatches.values()) {
    if (agg.created + agg.pending + agg.uploading + agg.saving + agg.paused > 0) return true
  }
  return false
}

export function resetUploadBatchesForTests(): void {
  uploadBatches.clear()
}
