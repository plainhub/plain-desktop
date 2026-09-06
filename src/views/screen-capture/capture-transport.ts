export const CAPTURE_PROTOCOL_VERSION = 1
export const FRAME_AVAILABLE_EVENT = 'screen-capture://frame-available'

export interface PhysicalPoint {
  x: number
  y: number
}

export interface PhysicalSize {
  width: number
  height: number
}

export interface LogicalPoint {
  x: number
  y: number
}

export interface LogicalSize {
  width: number
  height: number
}

export interface CaptureMonitorGeometry {
  id: string
  physicalOrigin: PhysicalPoint
  physicalSize: PhysicalSize
  logicalOrigin: LogicalPoint
  logicalSize: LogicalSize
  scaleFactor: number
}

export interface CaptureFrameDescriptor {
  sessionId: string
  monitor: CaptureMonitorGeometry
  width: number
  height: number
  stride: number
  pixelFormat: 'rgba8'
  byteLen: number
}

export interface CaptureFrameAvailable {
  sessionId: string
  overlayGeneration: number
  descriptor: CaptureFrameDescriptor
  canConfirm: boolean
}

export interface CaptureEvent<T> {
  payload: T
}

export type CaptureUnlisten = () => void
export type CaptureListen = <T = CaptureFrameAvailable>(
  event: string,
  handler: (event: CaptureEvent<T>) => void | Promise<void>
) => Promise<CaptureUnlisten>
export interface CaptureInvokeOptions {
  headers: Record<string, string>
}

export type CaptureInvoke = (command: string, args?: Record<string, unknown> | ArrayBuffer, options?: CaptureInvokeOptions) => Promise<unknown>

export interface CaptureTransportDependencies {
  overlayGeneration: number
  listen: CaptureListen
  invoke: CaptureInvoke
  present(image: ImageData, frame: CaptureFrameAvailable): Promise<void>
}

export interface CaptureTransport {
  recoverPendingFrame(): Promise<void>
  dispose(): void
}

function requirePositiveInteger(value: number, field: string): void {
  if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`invalid capture ${field}`)
}

export function parseOverlayGeneration(search: string): number {
  const value = Number(new URLSearchParams(search).get('overlayGeneration'))
  requirePositiveInteger(value, 'overlay generation')
  return value
}

export function captureOverlayWindowLabel(overlayGeneration: number): string {
  requirePositiveInteger(overlayGeneration, 'overlay generation')
  return `screen-capture-overlay-${overlayGeneration}`
}

export function frameBytesToImageData(descriptor: CaptureFrameDescriptor, buffer: ArrayBuffer): ImageData {
  requirePositiveInteger(descriptor.width, 'width')
  requirePositiveInteger(descriptor.height, 'height')
  requirePositiveInteger(descriptor.stride, 'stride')
  requirePositiveInteger(descriptor.byteLen, 'byte length')
  if (descriptor.pixelFormat !== 'rgba8') throw new Error('unsupported capture pixel format')

  const rowBytes = descriptor.width * 4
  if (!Number.isSafeInteger(rowBytes) || descriptor.stride < rowBytes) throw new Error('invalid capture stride')
  const expectedByteLength = descriptor.stride * descriptor.height
  if (!Number.isSafeInteger(expectedByteLength) || descriptor.byteLen !== expectedByteLength || buffer.byteLength !== expectedByteLength) {
    throw new Error('capture byte length does not match its descriptor')
  }

  if (descriptor.stride === rowBytes) {
    return new ImageData(new Uint8ClampedArray(buffer), descriptor.width, descriptor.height)
  }

  const packed = new Uint8ClampedArray(rowBytes * descriptor.height)
  const source = new Uint8Array(buffer)
  for (let row = 0; row < descriptor.height; row += 1) {
    packed.set(source.subarray(row * descriptor.stride, row * descriptor.stride + rowBytes), row * rowBytes)
  }
  return new ImageData(packed, descriptor.width, descriptor.height)
}

function errorDetail(error: unknown): string {
  const detail = error instanceof Error ? error.message : String(error)
  return detail.slice(0, 512)
}

export async function createCaptureTransport(deps: CaptureTransportDependencies): Promise<CaptureTransport> {
  requirePositiveInteger(deps.overlayGeneration, 'overlay generation')
  let disposed = false
  let processingSessionId: string | null = null
  let pendingFramePull: Promise<void> | null = null
  const handledSessionIds = new Set<string>()
  const handledSessionOrder: string[] = []
  const rememberHandledSession = (sessionId: string): void => {
    handledSessionIds.add(sessionId)
    handledSessionOrder.push(sessionId)
    if (handledSessionOrder.length > 16) {
      handledSessionIds.delete(handledSessionOrder.shift()!)
    }
  }
  const reportFailure = async (sessionId: string, code: string, detail: string): Promise<void> => {
    if (disposed) return
    try {
      await deps.invoke('screen_capture_fail', {
        sessionId,
        overlayGeneration: deps.overlayGeneration,
        code,
        detail,
      })
    } catch {
      // Native/window teardown may already have won. Tauri event handlers must
      // never leak a second rejection into the webview event bridge.
    }
  }

  const processFrame = async (payload: CaptureFrameAvailable): Promise<void> => {
    if (disposed) return
    // A fresh Windows overlay can observe the same publication through both
    // the event listener and the post-readiness pending-frame pull. Claim the
    // session before the first await so either ordering consumes it once.
    if (handledSessionIds.has(payload.sessionId)) return
    if (processingSessionId) {
      await reportFailure(payload.sessionId, 'overlay_busy', 'the capture overlay is already presenting another frame')
      return
    }

    processingSessionId = payload.sessionId
    rememberHandledSession(payload.sessionId)
    try {
      if (payload.overlayGeneration !== deps.overlayGeneration) {
        await reportFailure(payload.sessionId, 'stale_overlay_generation', 'capture frame belongs to another overlay generation')
        return
      }
      if (typeof payload.canConfirm !== 'boolean') throw new Error('capture target metadata is invalid')
      if (payload.descriptor.sessionId !== payload.sessionId) throw new Error('capture session metadata does not match the event')
      const raw = await deps.invoke('screen_capture_take_frame', {
        sessionId: payload.sessionId,
        overlayGeneration: deps.overlayGeneration,
      })
      if (disposed) return
      if (!(raw instanceof ArrayBuffer)) throw new Error('capture frame did not use binary IPC')
      const image = frameBytesToImageData(payload.descriptor, raw)
      await deps.present(image, payload)
      if (disposed) return
      await deps.invoke('screen_capture_frame_presented', {
        sessionId: payload.sessionId,
        overlayGeneration: deps.overlayGeneration,
      })
    } catch (error) {
      await reportFailure(payload.sessionId, 'frame_decode_failed', errorDetail(error))
    } finally {
      processingSessionId = null
    }
  }

  const recoverPendingFrame = async (): Promise<void> => {
    if (disposed) return
    if (pendingFramePull) return pendingFramePull
    pendingFramePull = (async () => {
      const pending = await deps.invoke('screen_capture_pending_frame', {
        overlayGeneration: deps.overlayGeneration,
      })
      if (pending) await processFrame(pending as CaptureFrameAvailable)
    })()
    try {
      await pendingFramePull
    } finally {
      pendingFramePull = null
    }
  }

  let unlisten: CaptureUnlisten = () => undefined
  const scheduledFrames: Array<{
    payload: CaptureFrameAvailable
    resolve(): void
  }> = []
  let frameTask: ReturnType<typeof globalThis.setTimeout> | null = null
  const armFrameTask = (): void => {
    if (frameTask !== null || !scheduledFrames.length) return
    frameTask = globalThis.setTimeout(() => {
      frameTask = null
      const next = scheduledFrames.shift()
      if (next) void processFrame(next.payload).then(next.resolve)
      armFrameTask()
    }, 0)
  }
  const scheduleFrame = (payload: CaptureFrameAvailable): Promise<void> => {
    if (disposed) return Promise.resolve()
    return new Promise((resolve) => {
      scheduledFrames.push({ payload, resolve })
      armFrameTask()
    })
  }
  // WebView2 cannot service renderer-to-host IPC re-entrantly from its native
  // event callback. Queue frame retrieval onto the next browser task after the
  // host event has returned. This is harmless on the other platforms.
  unlisten = await deps.listen(FRAME_AVAILABLE_EVENT, ({ payload }) => scheduleFrame(payload))

  try {
    await deps.invoke('screen_capture_ready', {
      overlayGeneration: deps.overlayGeneration,
      protocolVersion: CAPTURE_PROTOCOL_VERSION,
    })
    // The native capture may complete while the readiness response is still
    // crossing the webview boundary. Replay metadata without consuming bytes;
    // the session claim above deduplicates it against the queued event.
    await recoverPendingFrame()
  } catch (error) {
    unlisten()
    throw error
  }

  return {
    recoverPendingFrame,
    dispose() {
      disposed = true
      if (frameTask !== null) globalThis.clearTimeout(frameTask)
      frameTask = null
      for (const scheduled of scheduledFrames.splice(0)) scheduled.resolve()
      unlisten()
    },
  }
}
