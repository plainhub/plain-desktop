import { describe, expect, it, vi } from 'vitest'
import { createCaptureOverlaySession, type CaptureOverlayMount, type CaptureOverlayMountOptions } from '@/views/screen-capture/capture-overlay-session'
import type { CaptureFrameAvailable } from '@/views/screen-capture/capture-transport'

function frame(sessionId = 'session-1'): CaptureFrameAvailable {
  return {
    sessionId,
    overlayGeneration: 7,
    canConfirm: true,
    descriptor: {
      sessionId,
      monitor: {
        id: 'main',
        physicalOrigin: { x: 0, y: 0 },
        physicalSize: { width: 2, height: 2 },
        logicalOrigin: { x: 0, y: 0 },
        logicalSize: { width: 2, height: 2 },
        scaleFactor: 1,
      },
      width: 2,
      height: 2,
      stride: 8,
      pixelFormat: 'rgba8',
      byteLen: 16,
    },
  }
}

function harness() {
  const mounts: Array<{
    options: CaptureOverlayMountOptions
    setCanConfirm: ReturnType<typeof vi.fn>
    dispose: ReturnType<typeof vi.fn>
  }> = []
  const mount = vi.fn((_image: ImageData, options: CaptureOverlayMountOptions): CaptureOverlayMount => {
    const mounted = { options, setCanConfirm: vi.fn(), dispose: vi.fn() }
    mounts.push(mounted)
    return mounted
  })
  let result = 0
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown> | ArrayBuffer) => {
    if (command === 'screen_capture_submit_result') {
      result += 1
      return {
        sessionId: 'session-1',
        resultId: `result-${result}`,
        width: 2,
        height: 2,
        filename: 'capture.png',
        mimeType: 'image/png',
        byteLen: args instanceof ArrayBuffer ? args.byteLength : 0,
      }
    }
    return undefined
  })
  return {
    invoke,
    mount,
    mounts,
    session: createCaptureOverlaySession({ overlayGeneration: 7, invoke, mount }),
  }
}

const PNG_BYTES = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10, 1, 2, 3, 4])

function exportPayload() {
  return {
    png: new Blob([PNG_BYTES], { type: 'image/png' }),
    selection: { x: 0, y: 0, width: 2, height: 2 },
  }
}

describe('CaptureOverlaySession', () => {
  it('mounts frozen pixels synchronously without waiting on a hidden-window animation frame', async () => {
    const test = harness()
    const requestAnimationFrameSpy = vi.spyOn(window, 'requestAnimationFrame')
    const image = new ImageData(2, 2)

    await test.session.present(image, frame())

    expect(test.mount).toHaveBeenCalledWith(image, expect.objectContaining({ canConfirm: true }))
    expect(requestAnimationFrameSpy).not.toHaveBeenCalled()
  })

  it('forwards the native work area to the capture overlay', async () => {
    const test = harness()
    const visibleArea = { x: 0, y: 34, width: 1710, height: 997 }

    await test.session.present(new ImageData(2, 2), frame(), visibleArea)

    expect(test.mounts[0]!.options.visibleArea).toEqual(visibleArea)
  })

  it('disables confirm for a matching target-invalidated event and ignores stale metadata', async () => {
    const test = harness()
    await test.session.present(new ImageData(2, 2), frame())

    test.session.targetUnavailable({ sessionId: 'session-old', overlayGeneration: 7 })
    test.session.targetUnavailable({ sessionId: 'session-1', overlayGeneration: 6 })
    expect(test.mounts[0]!.setCanConfirm).not.toHaveBeenCalled()

    test.session.targetUnavailable({ sessionId: 'session-1', overlayGeneration: 7 })
    expect(test.mounts[0]!.setCanConfirm).toHaveBeenCalledWith(false)
  })

  it('remembers early target invalidation before a frame is mounted', async () => {
    const test = harness()
    test.session.targetUnavailable({ sessionId: 'session-1', overlayGeneration: 7 })

    await test.session.present(new ImageData(2, 2), frame())

    expect(test.mounts[0]!.options.canConfirm).toBe(false)
  })

  it('routes cancel through the authenticated overlay session', async () => {
    const test = harness()
    await test.session.present(new ImageData(2, 2), frame())

    await test.mounts[0]!.options.onCancel()

    expect(test.invoke).toHaveBeenCalledWith('screen_capture_cancel', {
      sessionId: 'session-1',
      overlayGeneration: 7,
    })
  })

  it('keeps confirm pending until the target acknowledges native completion', async () => {
    const test = harness()
    await test.session.present(new ImageData(2, 2), frame())
    let settled = false

    const sending = test.mounts[0]!.options.onExport('confirm', exportPayload()).then(() => {
      settled = true
    })
    await vi.waitFor(() => expect(test.invoke).toHaveBeenCalledWith('screen_capture_send_result', expect.any(Object)))
    await Promise.resolve()
    expect(settled).toBe(false)

    test.session.sessionEnded({ sessionId: 'session-1', overlayGeneration: 7, outcome: 'completed' })
    await expect(sending).resolves.toBeUndefined()
    expect(settled).toBe(true)
  })

  it('rejects a pending confirm when native reports a retryable delivery failure', async () => {
    const test = harness()
    await test.session.present(new ImageData(2, 2), frame())

    const sending = test.mounts[0]!.options.onExport('confirm', exportPayload())
    await vi.waitFor(() => expect(test.invoke).toHaveBeenCalledWith('screen_capture_send_result', expect.any(Object)))
    test.session.deliveryFailed({ sessionId: 'session-1', overlayGeneration: 7 })

    await expect(sending).rejects.toThrow(/delivery failed/i)
    expect(test.mounts[0]!.dispose).not.toHaveBeenCalled()
  })

  it('unmounts and clears frontend pixel ownership on a matching terminal event only', async () => {
    const test = harness()
    await test.session.present(new ImageData(2, 2), frame())

    test.session.sessionEnded({ sessionId: 'session-old', overlayGeneration: 7, outcome: 'cancelled' })
    expect(test.mounts[0]!.dispose).not.toHaveBeenCalled()

    test.session.sessionEnded({ sessionId: 'session-1', overlayGeneration: 7, outcome: 'cancelled' })
    expect(test.mounts[0]!.dispose).toHaveBeenCalledOnce()
  })

  it('disposes the previous frame before accepting another session', async () => {
    const test = harness()
    await test.session.present(new ImageData(2, 2), frame())

    await test.session.present(new ImageData(2, 2), frame('session-2'))

    expect(test.mounts[0]!.dispose).toHaveBeenCalledOnce()
    expect(test.mounts).toHaveLength(2)
  })

  it('disposes idempotently and rejects later presentation', async () => {
    const test = harness()
    await test.session.present(new ImageData(2, 2), frame())

    test.session.dispose()
    test.session.dispose()

    expect(test.mounts[0]!.dispose).toHaveBeenCalledOnce()
    await expect(test.session.present(new ImageData(2, 2), frame('session-2'))).rejects.toThrow(/disposed/i)
  })
})
