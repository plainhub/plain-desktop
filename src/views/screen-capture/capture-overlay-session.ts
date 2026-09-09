import type { CaptureExportPayload } from './ScreenCaptureOverlay.vue'
import type { CaptureExportAction } from './ScreenCaptureToolbar.vue'
import { createCaptureExportController, type CaptureExportControllerInvoke } from './capture-export-controller'
import type { CaptureFrameAvailable } from './capture-transport'
import type { SelectionRect } from './selection-model'

export interface CaptureTargetUnavailable {
  sessionId: string
  overlayGeneration: number
}

export interface CaptureOverlaySessionEnded {
  sessionId: string
  overlayGeneration: number
  outcome: 'completed' | 'cancelled' | 'saved' | 'copied' | 'failed'
}

export interface CaptureDeliveryFailed {
  sessionId: string
  overlayGeneration: number
}

export interface CaptureOverlayMountOptions {
  canConfirm: boolean
  visibleArea: SelectionRect | null
  onExport(action: CaptureExportAction, payload: CaptureExportPayload): Promise<void>
  onCancel(): Promise<void>
}

export interface CaptureOverlayMount {
  setCanConfirm(value: boolean): void
  dispose(): void
}

export interface CaptureOverlaySessionDependencies {
  overlayGeneration: number
  invoke: CaptureExportControllerInvoke
  mount(image: ImageData, options: CaptureOverlayMountOptions): CaptureOverlayMount
}

export interface CaptureOverlaySession {
  present(image: ImageData, frame: CaptureFrameAvailable, visibleArea?: SelectionRect | null): Promise<void>
  targetUnavailable(payload: CaptureTargetUnavailable): void
  deliveryFailed(payload: CaptureDeliveryFailed): void
  sessionEnded(payload: CaptureOverlaySessionEnded): void
  dispose(): void
}

interface PendingDelivery {
  promise: Promise<void>
  resolve(): void
  reject(error: Error): void
}

interface ActiveOverlay {
  sessionId: string
  mount: CaptureOverlayMount
  exportController: ReturnType<typeof createCaptureExportController>
  pendingDelivery: PendingDelivery | null
}

export function createCaptureOverlaySession(deps: CaptureOverlaySessionDependencies): CaptureOverlaySession {
  let active: ActiveOverlay | null = null
  let unavailableSessionId: string | null = null
  let disposed = false

  function clearActive(): void {
    if (!active) return
    const current = active
    active = null
    current.pendingDelivery?.reject(new Error('capture delivery ended before acknowledgment'))
    current.pendingDelivery = null
    current.exportController.dispose()
    current.mount.dispose()
  }

  function beginDelivery(): PendingDelivery {
    let resolvePromise!: () => void
    let rejectPromise!: (error: Error) => void
    const promise = new Promise<void>((resolve, reject) => {
      resolvePromise = resolve
      rejectPromise = reject
    })
    // Native delivery can fail while the publish command response is still
    // crossing IPC. Attach a handler immediately; the caller still observes
    // the same rejection when it awaits `promise` below.
    void promise.catch(() => undefined)
    let settled = false
    return {
      promise,
      resolve() {
        if (settled) return
        settled = true
        resolvePromise()
      },
      reject(error) {
        if (settled) return
        settled = true
        rejectPromise(error)
      },
    }
  }

  async function present(image: ImageData, frame: CaptureFrameAvailable, visibleArea: SelectionRect | null = null): Promise<void> {
    if (disposed) throw new Error('capture overlay session is disposed')
    if (frame.overlayGeneration !== deps.overlayGeneration || frame.descriptor.sessionId !== frame.sessionId) {
      throw new Error('capture frame session metadata is invalid')
    }
    clearActive()
    const exportController = createCaptureExportController({
      sessionId: frame.sessionId,
      overlayGeneration: deps.overlayGeneration,
      invoke: deps.invoke,
    })
    const pending = { current: null as PendingDelivery | null }
    const mount = deps.mount(image, {
      canConfirm: frame.canConfirm && unavailableSessionId !== frame.sessionId,
      visibleArea,
      onExport: async (action, payload) => {
        if (action !== 'confirm') return exportController.export(action, payload)
        if (pending.current) throw new Error('capture delivery is already pending')
        const delivery = beginDelivery()
        pending.current = delivery
        if (active?.sessionId === frame.sessionId) active.pendingDelivery = delivery
        try {
          await exportController.export(action, payload)
          await delivery.promise
        } finally {
          if (pending.current === delivery) pending.current = null
          if (active?.pendingDelivery === delivery) active.pendingDelivery = null
        }
      },
      onCancel: async () => {
        await deps.invoke('screen_capture_cancel', {
          sessionId: frame.sessionId,
          overlayGeneration: deps.overlayGeneration,
        })
      },
    })
    active = { sessionId: frame.sessionId, mount, exportController, pendingDelivery: pending.current }
    if (unavailableSessionId === frame.sessionId) unavailableSessionId = null
  }

  function targetUnavailable(payload: CaptureTargetUnavailable): void {
    if (disposed || payload.overlayGeneration !== deps.overlayGeneration) return
    if (active?.sessionId === payload.sessionId) {
      active.mount.setCanConfirm(false)
      active.pendingDelivery?.reject(new Error('capture delivery target is unavailable'))
      return
    }
    unavailableSessionId = payload.sessionId
  }

  function deliveryFailed(payload: CaptureDeliveryFailed): void {
    if (disposed || payload.overlayGeneration !== deps.overlayGeneration) return
    if (active?.sessionId !== payload.sessionId) return
    active.pendingDelivery?.reject(new Error('capture delivery failed'))
  }

  function sessionEnded(payload: CaptureOverlaySessionEnded): void {
    if (disposed || payload.overlayGeneration !== deps.overlayGeneration) return
    if (active?.sessionId === payload.sessionId) {
      if (payload.outcome === 'completed') active.pendingDelivery?.resolve()
      else active.pendingDelivery?.reject(new Error(`capture delivery ended with ${payload.outcome}`))
      clearActive()
    }
    if (unavailableSessionId === payload.sessionId) unavailableSessionId = null
  }

  return {
    present,
    targetUnavailable,
    deliveryFailed,
    sessionEnded,
    dispose() {
      if (disposed) return
      disposed = true
      unavailableSessionId = null
      clearActive()
    },
  }
}
