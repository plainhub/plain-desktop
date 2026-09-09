export interface RenderScope {
  main: boolean
  overlay: boolean
}

export type RenderFn = () => void

export class RenderScheduler {
  private pending: RenderScope | null = null
  private rafId = 0
  private mainRender: RenderFn = () => {}
  private overlayRender: RenderFn = () => {}
  private readonly flushBound: () => void
  private flushWaiters: Array<() => void> = []

  constructor() {
    this.flushBound = this.flush.bind(this)
  }

  setRenderers(main: RenderFn, overlay: RenderFn): void {
    this.mainRender = main
    this.overlayRender = overlay
  }

  requestRender(scope: Partial<RenderScope> = { main: true, overlay: true }): void {
    if (!this.pending) {
      this.pending = { main: false, overlay: false }
      this.rafId = requestAnimationFrame(this.flushBound)
    }
    if (scope.main) {
      this.pending.main = true
      this.pending.overlay = true
    } else if (scope.overlay) {
      this.pending.overlay = true
    }
  }

  requestMain(): void { this.requestRender({ main: true, overlay: true }) }
  requestOverlay(): void { this.requestRender({ main: false, overlay: true }) }

  requestMainAndWait(): Promise<void> {
    return new Promise(resolve => {
      this.flushWaiters.push(resolve)
      this.requestMain()
    })
  }

  private resolveFlushWaiters(): void {
    for (const resolve of this.flushWaiters.splice(0)) resolve()
  }

  private flush(): void {
    const scope = this.pending
    this.pending = null
    this.rafId = 0
    if (!scope) return
    try {
      if (scope.main) this.mainRender()
      if (scope.overlay) this.overlayRender()
    } finally {
      this.resolveFlushWaiters()
    }
  }

  dispose(): void {
    if (this.rafId) cancelAnimationFrame(this.rafId)
    this.rafId = 0
    this.pending = null
    this.mainRender = () => {}
    this.overlayRender = () => {}
    this.resolveFlushWaiters()
  }
}
