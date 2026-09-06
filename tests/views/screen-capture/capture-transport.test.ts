import { describe, expect, it, vi } from 'vitest'
import {
  captureOverlayWindowLabel,
  createCaptureTransport,
  frameBytesToImageData,
  parseOverlayGeneration,
  type CaptureFrameAvailable,
  type CaptureFrameDescriptor,
  type CaptureListen,
} from '@/views/screen-capture/capture-transport'

function descriptor(overrides: Partial<CaptureFrameDescriptor> = {}): CaptureFrameDescriptor {
  return {
    sessionId: 'session-1',
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
    ...overrides,
  }
}

function frameAvailable(overrides: Partial<CaptureFrameAvailable> = {}): CaptureFrameAvailable {
  return {
    sessionId: 'session-1',
    overlayGeneration: 7,
    canConfirm: true,
    descriptor: descriptor(),
    ...overrides,
  }
}

describe('frameBytesToImageData', () => {
  it('wraps tightly packed RGBA without a second full-frame allocation', () => {
    const buffer = new Uint8Array([1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]).buffer

    const image = frameBytesToImageData(descriptor(), buffer)

    expect(image.width).toBe(2)
    expect(image.height).toBe(2)
    expect(image.data.buffer).toBe(buffer)
    expect(Array.from(image.data)).toEqual([1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255])
  })

  it('rejects metadata and byte-length mismatches before rendering', () => {
    expect(() => frameBytesToImageData(descriptor({ byteLen: 15 }), new ArrayBuffer(16))).toThrow(/byte length/i)
    expect(() => frameBytesToImageData(descriptor({ pixelFormat: 'bgra8' as 'rgba8' }), new ArrayBuffer(16))).toThrow(/pixel format/i)
    expect(() => frameBytesToImageData(descriptor({ stride: 7 }), new ArrayBuffer(14))).toThrow(/stride/i)
    expect(() => frameBytesToImageData(descriptor({ width: Number.NaN }), new ArrayBuffer(16))).toThrow(/width/i)
    expect(() => frameBytesToImageData(descriptor({ height: Number.MAX_SAFE_INTEGER }), new ArrayBuffer(16))).toThrow(/byte length/i)
  })

  it('removes native row padding when the capture stride is wider than RGBA', () => {
    const buffer = new Uint8Array([1, 2, 3, 255, 4, 5, 6, 255, 99, 99, 99, 99, 7, 8, 9, 255, 10, 11, 12, 255, 88, 88, 88, 88]).buffer

    const image = frameBytesToImageData(descriptor({ stride: 12, byteLen: 24 }), buffer)

    expect(Array.from(image.data)).toEqual([1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255])
  })
})
describe('parseOverlayGeneration', () => {
  it('accepts one native-generated safe integer', () => {
    expect(parseOverlayGeneration('?overlayGeneration=7')).toBe(7)
  })

  it.each(['', '?overlayGeneration=0', '?overlayGeneration=-1', '?overlayGeneration=1.5', '?overlayGeneration=9007199254740992'])('rejects an absent or invalid generation in %s', (search) => {
    expect(() => parseOverlayGeneration(search)).toThrow(/generation/i)
  })
})

describe('captureOverlayWindowLabel', () => {
  it('binds the physical overlay identity to its native generation', () => {
    expect(captureOverlayWindowLabel(7)).toBe('screen-capture-overlay-7')
  })

  it.each([0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1])('rejects invalid generation %s', (generation) => {
    expect(() => captureOverlayWindowLabel(generation)).toThrow(/generation/i)
  })
})

describe('createCaptureTransport', () => {
  it('registers the listener before ready and acknowledges only after presentation', async () => {
    const order: string[] = []
    let onFrame!: (event: { payload: CaptureFrameAvailable }) => void | Promise<void>
    const listen: CaptureListen = vi.fn(async (_event, handler) => {
      order.push('listen')
      onFrame = handler
      return () => undefined
    })
    const bytes = new Uint8Array(16).buffer
    const invoke = vi.fn(async (command: string) => {
      order.push(command)
      if (command === 'screen_capture_take_frame') return bytes
      return undefined
    })
    const present = vi.fn(async () => {
      order.push('present')
    })

    await createCaptureTransport({ overlayGeneration: 7, listen, invoke, present })
    const handling = onFrame({ payload: frameAvailable() })

    expect(order).toEqual(['listen', 'screen_capture_ready', 'screen_capture_pending_frame'])
    await handling

    expect(order).toEqual(['listen', 'screen_capture_ready', 'screen_capture_pending_frame', 'screen_capture_take_frame', 'present', 'screen_capture_frame_presented'])
    expect(present).toHaveBeenCalledOnce()
    expect(invoke).toHaveBeenNthCalledWith(1, 'screen_capture_ready', { overlayGeneration: 7, protocolVersion: 1 })
    expect(invoke).toHaveBeenNthCalledWith(2, 'screen_capture_pending_frame', { overlayGeneration: 7 })
    expect(invoke).toHaveBeenNthCalledWith(3, 'screen_capture_take_frame', { overlayGeneration: 7, sessionId: 'session-1' })
    expect(invoke).toHaveBeenNthCalledWith(4, 'screen_capture_frame_presented', { overlayGeneration: 7, sessionId: 'session-1' })
  })

  it('pulls and deduplicates a frame that was pending when a fresh renderer became ready', async () => {
    const order: string[] = []
    const pending = frameAvailable()
    let onFrame!: (event: { payload: CaptureFrameAvailable }) => void | Promise<void>
    const invoke = vi.fn(async (command: string) => {
      order.push(command)
      if (command === 'screen_capture_pending_frame') return pending
      if (command === 'screen_capture_take_frame') return new Uint8Array(16).buffer
      return undefined
    })
    const present = vi.fn(async () => {
      order.push('present')
    })

    await createCaptureTransport({
      overlayGeneration: 7,
      listen: async (_event, handler) => {
        onFrame = handler
        return () => undefined
      },
      invoke,
      present,
    })

    expect(order).toEqual(['screen_capture_ready', 'screen_capture_pending_frame', 'screen_capture_take_frame', 'present', 'screen_capture_frame_presented'])
    expect(present).toHaveBeenCalledWith(expect.any(ImageData), pending)

    await onFrame({ payload: pending })
    expect(invoke.mock.calls.filter(([command]) => command === 'screen_capture_take_frame')).toHaveLength(1)
    expect(present).toHaveBeenCalledOnce()
  })

  it('pulls a frame that becomes pending after bootstrap surface restoration', async () => {
    let pending: CaptureFrameAvailable | null = null
    const invoke = vi.fn(async (command: string) => {
      if (command === 'screen_capture_pending_frame') return pending
      if (command === 'screen_capture_take_frame') return new Uint8Array(16).buffer
      return undefined
    })
    const present = vi.fn(async () => undefined)
    const transport = await createCaptureTransport({
      overlayGeneration: 7,
      listen: async () => () => undefined,
      invoke,
      present,
    })

    pending = frameAvailable()
    await transport.recoverPendingFrame()

    expect(present).toHaveBeenCalledWith(expect.any(ImageData), pending)
    expect(invoke.mock.calls.filter(([command]) => command === 'screen_capture_take_frame')).toHaveLength(1)
    expect(invoke.mock.calls.filter(([command]) => command === 'screen_capture_frame_presented')).toHaveLength(1)
  })

  it('reports decode failures without claiming that the frame was presented', async () => {
    let onFrame!: (event: { payload: CaptureFrameAvailable }) => void | Promise<void>
    const listen: CaptureListen = async (_event, handler) => {
      onFrame = handler
      return () => undefined
    }
    const invoke = vi.fn(async (command: string) => {
      if (command === 'screen_capture_take_frame') return new ArrayBuffer(3)
      return undefined
    })

    await createCaptureTransport({ overlayGeneration: 7, listen, invoke, present: vi.fn() })
    await onFrame({ payload: frameAvailable() })

    expect(invoke.mock.calls.map(([command]) => command)).toEqual(['screen_capture_ready', 'screen_capture_pending_frame', 'screen_capture_take_frame', 'screen_capture_fail'])
    expect(invoke).toHaveBeenLastCalledWith('screen_capture_fail', {
      code: 'frame_decode_failed',
      detail: 'capture byte length does not match its descriptor',
      overlayGeneration: 7,
      sessionId: 'session-1',
    })
  })

  it('contains a secondary native failure-report rejection inside the event boundary', async () => {
    let onFrame!: (event: { payload: CaptureFrameAvailable }) => void | Promise<void>
    const listen: CaptureListen = async (_event, handler) => {
      onFrame = handler
      return () => undefined
    }
    const invoke = vi.fn(async (command: string) => {
      if (command === 'screen_capture_take_frame') return new ArrayBuffer(3)
      if (command === 'screen_capture_fail') throw new Error('native failure channel closed')
      return undefined
    })

    await createCaptureTransport({ overlayGeneration: 7, listen, invoke, present: vi.fn() })

    await expect(onFrame({ payload: frameAvailable() })).resolves.toBeUndefined()
  })

  it('rejects malformed target eligibility before reading sensitive frame bytes', async () => {
    let onFrame!: (event: { payload: CaptureFrameAvailable }) => void | Promise<void>
    const listen: CaptureListen = async (_event, handler) => {
      onFrame = handler
      return () => undefined
    }
    const invoke = vi.fn(async () => undefined)

    await createCaptureTransport({ overlayGeneration: 7, listen, invoke, present: vi.fn() })
    await onFrame({ payload: frameAvailable({ canConfirm: 'yes' as unknown as boolean }) })

    expect(invoke.mock.calls.map(([command]) => command)).toEqual(['screen_capture_ready', 'screen_capture_pending_frame', 'screen_capture_fail'])
    expect(invoke).toHaveBeenLastCalledWith('screen_capture_fail', expect.objectContaining({ code: 'frame_decode_failed', detail: 'capture target metadata is invalid' }))
  })

  it('rejects stale frame events from another overlay generation', async () => {
    let onFrame!: (event: { payload: CaptureFrameAvailable }) => void | Promise<void>
    const listen: CaptureListen = async (_event, handler) => {
      onFrame = handler
      return () => undefined
    }
    const invoke = vi.fn(async () => undefined)
    const present = vi.fn()

    await createCaptureTransport({ overlayGeneration: 7, listen, invoke, present })
    await onFrame({ payload: frameAvailable({ overlayGeneration: 6 }) })

    expect(present).not.toHaveBeenCalled()
    expect(invoke).toHaveBeenLastCalledWith(
      'screen_capture_fail',
      expect.objectContaining({
        code: 'stale_overlay_generation',
        overlayGeneration: 7,
      })
    )
  })

  it('identifies the current overlay generation when rejecting a concurrent frame', async () => {
    let onFrame!: (event: { payload: CaptureFrameAvailable }) => void | Promise<void>
    let resolveFrame!: (value: ArrayBuffer) => void
    const framePromise = new Promise<ArrayBuffer>((resolve) => {
      resolveFrame = resolve
    })
    const listen: CaptureListen = async (_event, handler) => {
      onFrame = handler
      return () => undefined
    }
    const invoke = vi.fn(async (command: string) => {
      if (command === 'screen_capture_take_frame') return framePromise
      return undefined
    })

    await createCaptureTransport({ overlayGeneration: 7, listen, invoke, present: vi.fn() })
    const firstFrame = onFrame({ payload: frameAvailable() })
    await onFrame({ payload: frameAvailable({ sessionId: 'session-2', descriptor: descriptor({ sessionId: 'session-2' }) }) })

    expect(invoke).toHaveBeenCalledWith('screen_capture_fail', {
      sessionId: 'session-2',
      overlayGeneration: 7,
      code: 'overlay_busy',
      detail: 'the capture overlay is already presenting another frame',
    })

    resolveFrame(new Uint8Array(16).buffer)
    await firstFrame
  })

  it('deduplicates a retried event while the original binary read is unresolved', async () => {
    let onFrame!: (event: { payload: CaptureFrameAvailable }) => void | Promise<void>
    let resolveFrame!: (value: ArrayBuffer) => void
    const framePromise = new Promise<ArrayBuffer>((resolve) => {
      resolveFrame = resolve
    })
    const listen: CaptureListen = async (_event, handler) => {
      onFrame = handler
      return () => undefined
    }
    const invoke = vi.fn(async (command: string) => {
      if (command === 'screen_capture_take_frame') return framePromise
      return undefined
    })
    const present = vi.fn(async () => undefined)

    await createCaptureTransport({ overlayGeneration: 7, listen, invoke, present })
    const payload = frameAvailable()
    const firstDelivery = onFrame({ payload })
    await onFrame({ payload })
    resolveFrame(new Uint8Array(16).buffer)
    await firstDelivery

    expect(invoke.mock.calls.filter(([command]) => command === 'screen_capture_take_frame')).toHaveLength(1)
    expect(invoke.mock.calls.filter(([command]) => command === 'screen_capture_frame_presented')).toHaveLength(1)
    expect(invoke.mock.calls.filter(([command]) => command === 'screen_capture_fail')).toHaveLength(0)
    expect(present).toHaveBeenCalledOnce()
  })

  it('removes the listener when native readiness fails', async () => {
    const unlisten = vi.fn()
    const listen: CaptureListen = async () => unlisten
    const expected = new Error('native unavailable')
    const invoke = vi.fn(async () => {
      throw expected
    })

    await expect(createCaptureTransport({ overlayGeneration: 7, listen, invoke, present: vi.fn() })).rejects.toBe(expected)

    expect(unlisten).toHaveBeenCalledOnce()
  })

  it('does not read, present, or acknowledge a deferred frame after disposal', async () => {
    let onFrame!: (event: { payload: CaptureFrameAvailable }) => void | Promise<void>
    let resolveFrame!: (value: ArrayBuffer) => void
    const framePromise = new Promise<ArrayBuffer>((resolve) => {
      resolveFrame = resolve
    })
    const listen: CaptureListen = async (_event, handler) => {
      onFrame = handler
      return () => undefined
    }
    const invoke = vi.fn(async (command: string) => {
      if (command === 'screen_capture_take_frame') return framePromise
      return undefined
    })
    const present = vi.fn()
    const transport = await createCaptureTransport({ overlayGeneration: 7, listen, invoke, present })

    const handling = onFrame({ payload: frameAvailable() })
    transport.dispose()
    resolveFrame(new Uint8Array(16).buffer)
    await handling

    expect(present).not.toHaveBeenCalled()
    expect(invoke.mock.calls.map(([command]) => command)).toEqual(['screen_capture_ready', 'screen_capture_pending_frame'])
  })
})
