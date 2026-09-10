// Async chunk-merge client: `mergeChunksAsync` returns immediately and the
// result arrives as WS event `upload_merge_result` (type 38); `mergeStatus`
// is the polling fallback when the event is lost. Servers without the async
// mutation (old phones) are detected once per session and served through the
// legacy sync `mergeChunks` with a size-based timeout.

import type { IUploadItem } from '@/stores/temp'
import emitter from '@/plugins/eventbus'
import { gqlFetch, type GqlResult } from '../api/gql-client'
import { mergeChunksGQL, mergeChunksAsyncGQL } from '../api/mutation'
import { mergeStatusGQL } from '../api/query'

// Merging rewrites the whole file server-side (chunk reads + full copy, plus
// MediaStore scan on phones), which blows past the 30s default GraphQL
// timeout for multi-GB files. Budget 60s + 1s per 2MB (~2MB/s worst case),
// capped at 10 minutes.
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

export type MergeState =
  | { state: 'started' | 'merging' | 'none' }
  | { state: 'done'; value: string; size: number }
  | { state: 'failed'; error: string }

export function parseMergeState(raw: string): MergeState {
  if (raw === 'started' || raw === 'merging' || raw === 'none') return { state: raw }
  if (raw.startsWith('done:')) {
    const rest = raw.slice(5)
    const colon = rest.lastIndexOf(':')
    if (colon > 0) {
      return { state: 'done', value: rest.slice(0, colon), size: parseInt(rest.slice(colon + 1), 10) || 0 }
    }
    return { state: 'done', value: rest, size: 0 }
  }
  if (raw.startsWith('failed:')) {
    return { state: 'failed', error: raw.slice(7) }
  }
  return { state: 'none' }
}

// null = not probed yet; false = legacy server, skip the async mutation.
let asyncMergeSupported: boolean | null = null

export function resetAsyncMergeCapabilityForTests(): void {
  asyncMergeSupported = null
}

function hasUnknownFieldError(result: GqlResult, field: string): boolean {
  return (result.errors ?? []).some((e) => {
    const message = e.message || ''
    return message.includes(field) && /unknown field|Cannot query field/i.test(message)
  })
}

/** Merge `upload`'s chunks on the server; resolves only when the final
 *  value token ("fileName" or fidSuffix) and merged size are known. */
export async function requestMerge(upload: IUploadItem, args: Record<string, unknown>): Promise<IMergeOutcome> {
  if (asyncMergeSupported !== false) {
    try {
      const result = await gqlFetch(mergeChunksAsyncGQL, args, { timeout: 30_000 })
      if (hasUnknownFieldError(result, 'mergeChunksAsync')) {
        asyncMergeSupported = false
      } else {
        const parsed = parseMergeState(String(result.data?.mergeChunksAsync ?? ''))
        if (parsed.state === 'started' || parsed.state === 'merging') {
          asyncMergeSupported = true
          return await waitForMergeResult(upload, args.fileId as string)
        }
        if (parsed.state === 'done') {
          asyncMergeSupported = true
          return { value: parsed.value, size: parsed.size }
        }
        if (parsed.state === 'failed') {
          asyncMergeSupported = true
          return { error: parsed.error || 'Failed to merge chunks' }
        }
        // "none"/empty — unexpected reply, fall through to the sync mutation
      }
    } catch (e: any) {
      return { error: e.message || 'connection_timeout' }
    }
  }

  try {
    const result = await gqlFetch(mergeChunksGQL, args, { timeout: mergeRequestTimeout(upload.file.size) })
    if (result.errors?.length) {
      return { error: result.errors[0].message || 'Failed to merge chunks' }
    }
    const returned = result.data?.mergeChunks
    if (typeof returned !== 'string' || !returned) {
      return { error: 'Failed to merge chunks' }
    }
    const colonIdx = returned.lastIndexOf(':')
    if (colonIdx <= 0) return { value: returned, size: 0 }
    return { value: returned.substring(0, colonIdx), size: parseInt(returned.substring(colonIdx + 1), 10) || 0 }
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
          const parsed = parseMergeState(String(result.data?.mergeStatus ?? ''))
          if (parsed.state === 'done') {
            return finish({ value: parsed.value, size: parsed.size })
          }
          if (parsed.state === 'failed') {
            return finish({ error: parsed.error || 'Failed to merge chunks' })
          }
          if (parsed.state === 'none') {
            return finish({ error: 'Failed to merge chunks' })
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
