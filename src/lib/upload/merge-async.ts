// Async chunk-merge client: `mergeChunks` returns immediately with a
// MergeTask and the result arrives as WS event `upload_merge_result`
// (type 38); `mergeStatus` is the polling fallback when the event is lost.

import type { IUploadItem } from '@/stores/temp'
import emitter from '@/plugins/eventbus'
import { gqlFetch } from '../api/gql-client'
import { mergeChunksGQL, mergeAppFileChunksGQL } from '../api/mutation'
import { mergeStatusGQL } from '../api/query'

// Merging rewrites the whole file server-side (chunk reads + full copy, plus
// MediaStore scan on phones), which blows past the 30s default GraphQL
// timeout for multi-GB files — but the mutation itself returns immediately,
// so the only timeouts left are the short status polls.
export function mergeRequestTimeout(totalSize: number): number {
  return Math.min(600_000, 60_000 + Math.ceil(totalSize / (2 * 1024 * 1024)) * 1000)
}

const WATCHDOG_POLL_MS = 15_000
const STATUS_QUERY_TIMEOUT_MS = 30_000

export interface IMergeOutcome {
  value?: string
  size?: number
  error?: string
}

interface IMergeTask {
  status?: 'NONE' | 'STARTED' | 'MERGING' | 'DONE' | 'FAILED' | null
  value?: string | null
  mergedSize?: number | null
  error?: string | null
}

function taskOutcome(task: IMergeTask | null | undefined): IMergeOutcome | 'pending' {
  if (!task || task.status === 'NONE' || task.status === 'STARTED' || task.status === 'MERGING') return 'pending'
  if (task.status === 'DONE') return { value: String(task.value ?? ''), size: Number(task.mergedSize) || 0 }
  return { error: task.error || 'Failed to merge chunks' }
}

/** Merge `upload`'s chunks on the server; resolves only when the final
 *  value token ("fileName" or fidSuffix) and merged size are known. */
export async function requestMerge(upload: IUploadItem, args: Record<string, unknown>): Promise<IMergeOutcome> {
  // App-store uploads go through the dedicated mergeAppFileChunks mutation.
  const doc = upload.isAppFile ? mergeAppFileChunksGQL : mergeChunksGQL
  const field = upload.isAppFile ? 'mergeAppFileChunks' : 'mergeChunks'
  try {
    const result = await gqlFetch(doc, args, { timeout: 30_000 })
    if (result.errors?.length) {
      return { error: result.errors[0].message || 'Failed to merge chunks' }
    }
    const outcome = taskOutcome(result.data?.[field] as IMergeTask)
    if (outcome !== 'pending') return outcome
    return await waitForMergeResult(upload, args.fileId as string)
  } catch (e: any) {
    return { error: e.message || 'connection_timeout' }
  }
}

function waitForMergeResult(upload: IUploadItem, fileId: string): Promise<IMergeOutcome> {
  return new Promise<IMergeOutcome>((resolve) => {
    let settled = false
    let watchdog: ReturnType<typeof setTimeout> | undefined
    let pauseProbe: ReturnType<typeof setInterval> | undefined
    // Deadline as a poll budget (not Date.now) so fake-timer tests can drive
    // it deterministically; ~mergeRequestTimeout worth of 15s status polls.
    let pollsLeft = Math.max(1, Math.ceil(mergeRequestTimeout(upload.file.size) / WATCHDOG_POLL_MS))

    const finish = (outcome: IMergeOutcome) => {
      if (settled) return
      settled = true
      if (watchdog) clearTimeout(watchdog)
      if (pauseProbe) clearInterval(pauseProbe)
      emitter.off('upload_merge_result', onMergeEvent)
      resolve(outcome)
    }

    function onMergeEvent(payload: { fileId?: string; ok?: boolean; value?: string; mergedSize?: number; error?: string }) {
      if (!payload || payload.fileId !== fileId) return
      if (payload.ok) {
        finish({ value: String(payload.value ?? ''), size: Number(payload.mergedSize) || 0 })
      } else {
        finish({ error: payload.error || 'Failed to merge chunks' })
      }
    }

    emitter.on('upload_merge_result', onMergeEvent)

    pauseProbe = setInterval(() => {
      if (upload.status === 'paused' || upload.status === 'canceled') {
        finish({ error: 'Upload paused' })
      }
    }, 1000)

    const armWatchdog = () => {
      watchdog = setTimeout(async () => {
        try {
          const result = await gqlFetch(mergeStatusGQL, { fileId }, { timeout: STATUS_QUERY_TIMEOUT_MS })
          const outcome = taskOutcome(result.data?.mergeStatus as IMergeTask)
          if (outcome !== 'pending') {
            return finish(outcome)
          }
        } catch {
          // status query itself failed — try again on the next tick
        }
        if (--pollsLeft <= 0) {
          return finish({ error: 'connection_timeout' })
        }
        armWatchdog()
      }, WATCHDOG_POLL_MS)
    }
    armWatchdog()
  })
}
