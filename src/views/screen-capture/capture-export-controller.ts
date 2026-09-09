import type { CaptureExportPayload } from './ScreenCaptureOverlay.vue'
import type { CaptureExportAction } from './ScreenCaptureToolbar.vue'
import type { CaptureInvokeOptions } from './capture-transport'

const MAX_PNG_BYTES = 160 * 1024 * 1024
const PNG_SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10] as const

const RESULT_SESSION_HEADER = 'x-plain-capture-session-id'
const RESULT_GENERATION_HEADER = 'x-plain-capture-overlay-generation'
const RESULT_WIDTH_HEADER = 'x-plain-capture-width'
const RESULT_HEIGHT_HEADER = 'x-plain-capture-height'

export type CaptureExportControllerInvoke = (command: string, args?: Record<string, unknown> | ArrayBuffer, options?: CaptureInvokeOptions) => Promise<unknown>

export interface CaptureExportControllerDependencies {
  sessionId: string
  overlayGeneration: number
  invoke: CaptureExportControllerInvoke
}

interface CaptureResultDescriptor {
  sessionId: string
  resultId: string
  width: number
  height: number
  filename: string
  mimeType: 'image/png'
  byteLen: number
}

export interface CaptureExportController {
  export(action: CaptureExportAction, payload: CaptureExportPayload): Promise<void>
  dispose(): void
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function validIdentifier(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= 256 && value.trim() === value
}

function validPositiveInteger(value: unknown, maximum = Number.MAX_SAFE_INTEGER): value is number {
  return Number.isSafeInteger(value) && (value as number) > 0 && (value as number) <= maximum
}

function validFilename(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= 255 && value !== '.' && value !== '..' && !/[\\/\0]/.test(value) && value.toLowerCase().endsWith('.png')
}

function validateDescriptor(value: unknown, expected: { sessionId: string; width: number; height: number; byteLen: number }): CaptureResultDescriptor {
  if (
    !isRecord(value) ||
    value.sessionId !== expected.sessionId ||
    !validIdentifier(value.resultId) ||
    value.width !== expected.width ||
    value.height !== expected.height ||
    !validFilename(value.filename) ||
    value.mimeType !== 'image/png' ||
    value.byteLen !== expected.byteLen ||
    !validPositiveInteger(value.byteLen, MAX_PNG_BYTES)
  ) {
    throw new Error('native capture result descriptor is invalid')
  }
  return value as unknown as CaptureResultDescriptor
}

function validatePng(blob: Blob, bytes: ArrayBuffer): void {
  if (blob.type !== 'image/png' || bytes.byteLength < PNG_SIGNATURE.length || bytes.byteLength > MAX_PNG_BYTES) {
    throw new Error('capture export is not a valid PNG payload')
  }
  const prefix = new Uint8Array(bytes, 0, PNG_SIGNATURE.length)
  if (PNG_SIGNATURE.some((byte, index) => prefix[index] !== byte)) {
    throw new Error('capture export is not a valid PNG payload')
  }
}

export function createCaptureExportController(deps: CaptureExportControllerDependencies): CaptureExportController {
  if (!validIdentifier(deps.sessionId) || !validPositiveInteger(deps.overlayGeneration)) {
    throw new Error('capture export session is invalid')
  }

  let currentResult: CaptureResultDescriptor | null = null
  let busy = false
  let disposed = false

  async function failProtocol(): Promise<void> {
    try {
      await deps.invoke('screen_capture_fail', {
        sessionId: deps.sessionId,
        overlayGeneration: deps.overlayGeneration,
        code: 'protocol_mismatch',
        detail: 'native capture result descriptor is invalid',
      })
    } catch {
      // Preserve the original protocol error. Native command validation and
      // lifecycle hooks remain the final cleanup backstop if this IPC fails.
    }
  }

  async function discardCurrent(): Promise<void> {
    if (!currentResult) return
    await deps.invoke('screen_capture_discard_result', {
      sessionId: deps.sessionId,
      resultId: currentResult.resultId,
      overlayGeneration: deps.overlayGeneration,
    })
    currentResult = null
  }

  async function exportResult(action: CaptureExportAction, payload: CaptureExportPayload): Promise<void> {
    if (disposed) throw new Error('capture export controller is disposed')
    if (busy) throw new Error('a capture export is already in progress')
    busy = true
    try {
      await discardCurrent()
      const bytes = await payload.png.arrayBuffer()
      validatePng(payload.png, bytes)

      const rawDescriptor = await deps.invoke('screen_capture_submit_result', bytes, {
        headers: {
          [RESULT_SESSION_HEADER]: deps.sessionId,
          [RESULT_GENERATION_HEADER]: String(deps.overlayGeneration),
          [RESULT_WIDTH_HEADER]: String(payload.selection.width),
          [RESULT_HEIGHT_HEADER]: String(payload.selection.height),
        },
      })
      let descriptor: CaptureResultDescriptor
      try {
        descriptor = validateDescriptor(rawDescriptor, {
          sessionId: deps.sessionId,
          width: payload.selection.width,
          height: payload.selection.height,
          byteLen: bytes.byteLength,
        })
      } catch (error) {
        await failProtocol()
        throw error
      }
      currentResult = descriptor

      const args = {
        sessionId: deps.sessionId,
        resultId: descriptor.resultId,
        overlayGeneration: deps.overlayGeneration,
      }
      if (action === 'confirm') {
        await deps.invoke('screen_capture_send_result', args)
        return
      }
      if (action === 'copy') {
        await deps.invoke('screen_capture_copy_result', args)
        currentResult = null
        return
      }
      const outcome = await deps.invoke('screen_capture_save_result', args)
      if (outcome === 'saved') currentResult = null
      else if (outcome !== 'cancelled') throw new Error('native capture save returned an invalid outcome')
    } finally {
      busy = false
    }
  }

  return {
    export: exportResult,
    dispose() {
      disposed = true
      currentResult = null
    },
  }
}
