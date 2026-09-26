import { afterEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/hooks/app-socket', () => ({
  sendAppWsBytes: vi.fn(() => true),
  sendAppWsJson: vi.fn(() => true),
}))

import { ScreenMirrorControl, type ScreenMirrorControlEvent } from '@/views/screen-mirror/screen-mirror-control'
import { TOUCH_ACTION_DOWN, TOUCH_ACTION_MOVE, TOUCH_ACTION_UP, type TouchSample } from '@/views/screen-mirror/touch-frame'

// Canvas rect (0,100,400,800): normalized = (client - rect.origin) / 400|800.
const CANVAS_RECT = new DOMRect(0, 100, 400, 800)
const OVERLAY_RECT = new DOMRect(0, 0, 400, 900)

function setup() {
  const canvas = document.createElement('canvas')
  canvas.width = 400
  canvas.height = 800
  const overlay = document.createElement('div')
  document.body.appendChild(canvas)
  document.body.appendChild(overlay)
  const canvasRectSpy = vi.spyOn(canvas, 'getBoundingClientRect').mockReturnValue(CANVAS_RECT)
  const overlayRectSpy = vi.spyOn(overlay, 'getBoundingClientRect').mockReturnValue(OVERLAY_RECT)
  // synthetic pointers are not "active", real setPointerCapture would throw
  overlay.setPointerCapture = () => {}

  const sentControl: ScreenMirrorControlEvent[] = []
  const sampleBatches: TouchSample[][] = []
  const control = new ScreenMirrorControl(
    canvas,
    overlay,
    (e) => sentControl.push(e),
    (s) => sampleBatches.push(s),
  )
  control.setEnabled(true)
  control.setupListeners()

  const pointer = (type: string, opts: PointerEventInit & { pointerId: number }) =>
    overlay.dispatchEvent(new PointerEvent(type, { bubbles: true, ...opts }))
  const down = (pointerId = 1, clientX = 200, clientY = 500, button = 0) =>
    pointer('pointerdown', { pointerId, clientX, clientY, button, isPrimary: pointerId === 1 })
  const move = (pointerId = 1, clientX = 200, clientY = 500) =>
    pointer('pointermove', { pointerId, clientX, clientY })
  const up = (pointerId = 1, clientX = 200, clientY = 500) =>
    pointer('pointerup', { pointerId, clientX, clientY })

  const cleanup = () => {
    control.destroy()
    canvas.remove()
    overlay.remove()
  }
  return {
    control,
    sentControl,
    samples: () => sampleBatches.flat(),
    pointer,
    down,
    move,
    up,
    overlay,
    canvasRectSpy,
    overlayRectSpy,
    cleanup,
  }
}

describe('ScreenMirrorControl', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('injects the contact immediately on pointerdown — no tap/drag classification delay', () => {
    const t = setup()
    try {
      t.down(1, 200, 500)
      expect(t.samples()).toEqual([
        { action: TOUCH_ACTION_DOWN, pointerId: 1, x: 0.5, y: 0.5, dtMs: 0 },
      ])
      expect(t.sentControl).toEqual([])
    } finally {
      t.cleanup()
    }
  })

  it('a tap is a natural short down+up — no client-side TAP action is sent', () => {
    const t = setup()
    try {
      t.down(1, 200, 500)
      t.up(1, 201, 501)
      expect(t.sentControl).toEqual([])
      expect(t.samples().map((s) => s.action)).toEqual([TOUCH_ACTION_DOWN, TOUCH_ACTION_UP])
    } finally {
      t.cleanup()
    }
  })

  it('emits every raw move sample — no throttling, no latest-point merging', () => {
    const t = setup()
    try {
      t.down(1, 200, 500)
      t.move(1, 210, 500)
      t.move(1, 220, 510)
      t.move(1, 230, 520)
      const actions = t.samples().map((s) => s.action)
      expect(actions).toEqual([TOUCH_ACTION_DOWN, TOUCH_ACTION_MOVE, TOUCH_ACTION_MOVE, TOUCH_ACTION_MOVE])
      const last = t.samples().at(-1)!
      expect(last.x).toBeCloseTo(230 / 400, 6)
      expect(last.y).toBeCloseTo((520 - 100) / 800, 6)
    } finally {
      t.cleanup()
    }
  })

  it('tracks multiple pointers for multi-touch', () => {
    const t = setup()
    try {
      t.down(1, 100, 500)
      t.down(2, 300, 500)
      const downs = t.samples().filter((s) => s.action === TOUCH_ACTION_DOWN)
      expect(downs.map((s) => s.pointerId)).toEqual([1, 2])
      expect(downs[0].x).toBeCloseTo(0.25, 6)
      expect(downs[1].x).toBeCloseTo(0.75, 6)
    } finally {
      t.cleanup()
    }
  })

  it('clamps out-of-picture coordinates to the screen edges instead of dropping', () => {
    const t = setup()
    try {
      t.down(1, 500, 500) // right of the canvas rect
      expect(t.samples()[0].x).toBe(1)
      t.up(1, 500, 500)
      t.down(2, -100, 500) // left of the canvas rect
      expect(t.samples().at(-1)!.x).toBe(0)
    } finally {
      t.cleanup()
    }
  })

  it('synthesizes the missed pointerup on lostpointercapture (dead-pointer recovery)', () => {
    const t = setup()
    try {
      t.down(1, 200, 500)
      t.pointer('lostpointercapture', { pointerId: 1 })
      expect(t.samples().map((s) => s.action)).toEqual([TOUCH_ACTION_DOWN, TOUCH_ACTION_UP])
      // the pointer table is cleared: a new down with the same id starts fresh
      const before = t.samples().length
      t.down(1, 200, 500)
      expect(t.samples().length).toBe(before + 1)
      expect(t.samples().at(-1)!.action).toBe(TOUCH_ACTION_DOWN)
    } finally {
      t.cleanup()
    }
  })

  it('releases a stale pointer before reusing its id for a new gesture', () => {
    const t = setup()
    try {
      t.down(1, 200, 500)
      t.down(1, 300, 500)
      const actions = t.samples().map((s) => s.action)
      expect(actions).toEqual([TOUCH_ACTION_DOWN, TOUCH_ACTION_UP, TOUCH_ACTION_DOWN])
      expect(t.samples().at(-1)!.x).toBeCloseTo(0.75, 6)
    } finally {
      t.cleanup()
    }
  })

  it('cancels in-flight gestures when disabled and blocks new ones', () => {
    const t = setup()
    try {
      t.down(1, 200, 500)
      t.control.setEnabled(false)
      expect(t.samples().at(-1)!.action).toBe(3) // CANCEL
      const before = t.samples().length
      t.down(1, 200, 500)
      t.move(1, 210, 500)
      expect(t.samples().length).toBe(before)
    } finally {
      t.cleanup()
    }
  })

  it('right button sends BACK, middle button sends HOME — no touch samples', () => {
    const t = setup()
    try {
      t.down(1, 200, 500, 2)
      expect(t.sentControl.map((e) => e.action)).toEqual(['BACK'])
      t.down(1, 200, 500, 1)
      expect(t.sentControl.map((e) => e.action)).toEqual(['BACK', 'HOME'])
      expect(t.samples()).toEqual([])
    } finally {
      t.cleanup()
    }
  })

  it('plain wheel sends SCROLL and consumes the event', () => {
    const t = setup()
    try {
      const ev = new WheelEvent('wheel', {
        deltaX: 0,
        deltaY: 120,
        clientX: 200,
        clientY: 500,
        cancelable: true,
      })
      t.overlay.dispatchEvent(ev)
      expect(ev.defaultPrevented).toBe(true)
      expect(t.sentControl).toEqual([
        { action: 'SCROLL', x: 0.5, y: 0.5, deltaX: 0, deltaY: 120 },
      ])
    } finally {
      t.cleanup()
    }
  })

  it('ctrl+wheel (trackpad pinch) synthesizes a two-finger pinch instead of scrolling', () => {
    const t = setup()
    try {
      t.overlay.dispatchEvent(
        new WheelEvent('wheel', { deltaY: 100, ctrlKey: true, clientX: 200, clientY: 500, cancelable: true }),
      )
      const downs = t.samples().filter((s) => s.action === TOUCH_ACTION_DOWN)
      expect(downs.map((s) => s.pointerId)).toEqual([240, 241])
      expect(t.sentControl).toEqual([])
    } finally {
      t.cleanup()
    }
  })

  it('a real pointerdown collapses the virtual pinch so they never merge into three fingers', () => {
    const t = setup()
    try {
      t.overlay.dispatchEvent(
        new WheelEvent('wheel', { deltaY: 100, ctrlKey: true, clientX: 200, clientY: 500, cancelable: true }),
      )
      t.down(1, 200, 500)
      expect(t.samples().map((s) => [s.pointerId, s.action])).toEqual([
        [240, TOUCH_ACTION_DOWN],
        [241, TOUCH_ACTION_DOWN],
        [240, TOUCH_ACTION_UP],
        [241, TOUCH_ACTION_UP],
        [1, TOUCH_ACTION_DOWN],
      ])
    } finally {
      t.cleanup()
    }
  })

  it('refreshes the render rects once per gesture start, never on the move hot path', () => {
    const t = setup()
    try {
      t.down(1, 200, 500)
      t.move(1, 210, 500)
      t.move(1, 220, 500)
      t.up(1, 220, 500)
      expect(t.canvasRectSpy).toHaveBeenCalledTimes(1)
      expect(t.overlayRectSpy).toHaveBeenCalledTimes(1)
      // next gesture start refreshes again
      t.down(1, 200, 500)
      expect(t.canvasRectSpy).toHaveBeenCalledTimes(2)
    } finally {
      t.cleanup()
    }
  })

  it('destroy cancels in-flight gestures and detaches listeners', () => {
    const t = setup()
    try {
      t.down(1, 200, 500)
      t.cleanup() // calls control.destroy()
      expect(t.samples().at(-1)!.action).toBe(3) // CANCEL
      const before = t.samples().length
      t.down(1, 200, 500)
      expect(t.samples().length).toBe(before)
    } finally {
      t.cleanup()
    }
  })
})
