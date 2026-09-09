import { describe, expect, it, vi } from 'vitest'
import { getCaptureOverlayWorkArea } from '@/views/screen-capture/capture-work-area'
import type { CaptureInvoke } from '@/views/screen-capture/capture-transport'

describe('capture overlay work area', () => {
  it('maps the authenticated native CSS rectangle into toolbar coordinates', async () => {
    const invoke = vi.fn(async () => ({
      origin: { x: 0, y: 34 },
      size: { width: 1710, height: 997 },
    })) as CaptureInvoke

    await expect(getCaptureOverlayWorkArea(invoke, 7)).resolves.toEqual({ x: 0, y: 34, width: 1710, height: 997 })
    expect(invoke).toHaveBeenCalledWith('screen_capture_overlay_work_area', { overlayGeneration: 7 })
  })

  it('falls back safely when native geometry is absent, malformed, or unavailable', async () => {
    const absent = vi.fn(async () => null) as CaptureInvoke
    const malformed = vi.fn(async () => ({ origin: { x: 0, y: 0 }, size: { width: 100, height: 0 } })) as CaptureInvoke
    const unavailable = vi.fn(async () => Promise.reject(new Error('unsupported'))) as CaptureInvoke

    await expect(getCaptureOverlayWorkArea(absent, 7)).resolves.toBeNull()
    await expect(getCaptureOverlayWorkArea(malformed, 7)).resolves.toBeNull()
    await expect(getCaptureOverlayWorkArea(unavailable, 7)).resolves.toBeNull()
  })
})
