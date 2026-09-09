import type { CaptureInvoke } from './capture-transport'
import type { SelectionRect } from './selection-model'

interface NativeCssRect {
  origin?: { x?: unknown; y?: unknown }
  size?: { width?: unknown; height?: unknown }
}

function finiteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value)
}

export async function getCaptureOverlayWorkArea(invoke: CaptureInvoke, overlayGeneration: number): Promise<SelectionRect | null> {
  try {
    const value = (await invoke('screen_capture_overlay_work_area', { overlayGeneration })) as NativeCssRect | null
    const x = value?.origin?.x
    const y = value?.origin?.y
    const width = value?.size?.width
    const height = value?.size?.height
    if (!finiteNumber(x) || !finiteNumber(y) || !finiteNumber(width) || !finiteNumber(height) || width <= 0 || height <= 0) return null
    return { x, y, width, height }
  } catch {
    // Work-area detection is an enhancement. Keep capture available and let
    // the overlay fall back to browser screen geometry if the host cannot
    // provide it.
    return null
  }
}
