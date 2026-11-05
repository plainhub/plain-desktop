export function createRefreshScheduler(onRefresh: () => void, opts: { delay?: number; maxWait?: number } = {}) {
  const delay = opts.delay ?? 2000
  const maxWait = opts.maxWait ?? 10000
  let timer: ReturnType<typeof setTimeout> | undefined
  let firstPendingAt: number | undefined

  const run = () => {
    timer = undefined
    firstPendingAt = undefined
    onRefresh()
  }

  const schedule = () => {
    firstPendingAt ??= Date.now()
    if (timer !== undefined) clearTimeout(timer)
    const elapsed = Date.now() - firstPendingAt
    timer = setTimeout(run, Math.max(0, Math.min(delay, maxWait - elapsed)))
  }

  return {
    schedule,
    dispose() {
      if (timer !== undefined) clearTimeout(timer)
      timer = undefined
      firstPendingAt = undefined
    },
  }
}
