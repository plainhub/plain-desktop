import type { IUploadItem } from '@/stores/temp'

export interface UploadRefreshScheduler {
  schedule: () => void
  onUploadDone: (r: IUploadItem) => void
  dispose: () => void
}

// Coalesces the per-file `upload_task_done` storm into rare refreshes. A huge
// directory upload completes files many times per second; refreshing a listing
// per file is quadratic overall and starves the upload connections, so we
// debounce (quiet period) with a max-wait cap (sustained storms still refresh
// periodically instead of being suppressed for the whole upload).
export function createUploadRefreshScheduler(onRefresh: () => void, opts: { delay?: number; maxWait?: number } = {}): UploadRefreshScheduler {
  const delay = opts.delay ?? 2000
  const maxWait = opts.maxWait ?? 10000
  let timer: ReturnType<typeof setTimeout> | undefined
  let firstPendingAt = 0

  const run = () => {
    timer = undefined
    firstPendingAt = 0
    onRefresh()
  }

  const schedule = () => {
    if (!firstPendingAt) firstPendingAt = Date.now()
    if (timer) clearTimeout(timer)
    const elapsed = Date.now() - firstPendingAt
    timer = setTimeout(run, Math.max(0, Math.min(delay, maxWait - elapsed)))
  }

  return {
    schedule,
    onUploadDone(r: IUploadItem) {
      if (r.status === 'done') schedule()
    },
    dispose() {
      if (timer) clearTimeout(timer)
      timer = undefined
      firstPendingAt = 0
    },
  }
}
