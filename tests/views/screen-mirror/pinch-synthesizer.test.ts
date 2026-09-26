import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { PinchSynthesizer } from '@/views/screen-mirror/pinch-synthesizer'
import { TOUCH_ACTION_DOWN, TOUCH_ACTION_MOVE, TOUCH_ACTION_UP, type TouchSample } from '@/views/screen-mirror/touch-frame'
import type { RectPair } from '@/views/screen-mirror/view-rects'

// Canvas 400×800: the initial half-distance is 60px / 400 = 0.15 normalized.
const RECTS: RectPair = {
  canvasRect: new DOMRect(0, 0, 400, 800),
  overlayRect: new DOMRect(0, 0, 400, 800),
}
const INITIAL_RADIUS = 60 / 400

describe('PinchSynthesizer', () => {
  let batches: TouchSample[][]
  let synth: PinchSynthesizer

  const all = () => batches.flat()
  const byId = (id: number) => all().filter((s) => s.pointerId === id)

  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'setInterval', 'clearTimeout', 'clearInterval', 'performance'] })
    batches = []
    const overlay = document.createElement('div')
    synth = new PinchSynthesizer(overlay, (s) => batches.push(s))
  })

  afterEach(() => {
    synth.end(true)
    vi.useRealTimers()
  })

  it('plants two virtual fingers (pointerId 240/241) on the first Ctrl+wheel', () => {
    synth.onWheel(0.5, 0.5, 100, 0, RECTS)
    const downs = all().filter((s) => s.action === TOUCH_ACTION_DOWN)
    expect(downs.map((s) => s.pointerId)).toEqual([240, 241])
    // horizontal spread ±0.15 around the anchor
    expect(downs[0].x).toBeCloseTo(0.5 - INITIAL_RADIUS, 6)
    expect(downs[1].x).toBeCloseTo(0.5 + INITIAL_RADIUS, 6)
    expect(downs[0].y).toBeCloseTo(0.5, 6)
  })

  it('scroll down closes the fingers (zoom out) and lifts them after idle', () => {
    synth.onWheel(0.5, 0.5, 100, 0, RECTS)
    const target = INITIAL_RADIUS * Math.exp(-100 * 0.002)
    vi.advanceTimersByTime(3000)
    const ups = byId(240).filter((s) => s.action === TOUCH_ACTION_UP)
    expect(ups).toHaveLength(1)
    // converged to the exponential target within the 0.0015 settle tolerance
    expect(ups[0].x).toBeGreaterThanOrEqual(0.5 - target - 0.002)
    expect(ups[0].x).toBeLessThanOrEqual(0.5 - target + 0.002)
    // zoom out really closed the fingers
    expect(ups[0].x).toBeGreaterThan(0.5 - INITIAL_RADIUS)
    expect(byId(241).filter((s) => s.action === TOUCH_ACTION_UP)).toHaveLength(1)
  })

  it('scroll up spreads the fingers (zoom in)', () => {
    synth.onWheel(0.5, 0.5, -100, 0, RECTS)
    const target = INITIAL_RADIUS * Math.exp(100 * 0.002)
    vi.advanceTimersByTime(3000)
    const ups = byId(240).filter((s) => s.action === TOUCH_ACTION_UP)
    expect(ups[0].x).toBeLessThan(0.5 - INITIAL_RADIUS)
    void target
  })

  it('streams MOVE pairs at the tick cadence while converging', () => {
    synth.onWheel(0.5, 0.5, 100, 0, RECTS)
    const movesBefore = byId(240).filter((s) => s.action === TOUCH_ACTION_MOVE).length
    vi.advanceTimersByTime(16)
    const movesAfter = byId(240).filter((s) => s.action === TOUCH_ACTION_MOVE).length
    expect(movesAfter).toBeGreaterThan(movesBefore)
    // both fingers move symmetrically
    const lastA = byId(240).filter((s) => s.action === TOUCH_ACTION_MOVE).at(-1)!
    const lastB = byId(241).filter((s) => s.action === TOUCH_ACTION_MOVE).at(-1)!
    expect(lastA.x + lastB.x).toBeCloseTo(1.0, 6)
  })

  it('clamps a single huge wheel delta instead of jumping the full range', () => {
    synth.onWheel(0.5, 0.5, 1e6, 0, RECTS)
    const target = INITIAL_RADIUS * 0.6 // factor clamped to the 0.6 floor
    vi.advanceTimersByTime(3000)
    const ups = byId(240).filter((s) => s.action === TOUCH_ACTION_UP)
    expect(ups[0].x).toBeGreaterThanOrEqual(0.5 - target - 0.002)
    expect(ups[0].x).toBeLessThanOrEqual(0.5 - target + 0.002)
  })

  it('end(false) lifts the fingers, end(true) cancels, and end is idempotent', () => {
    synth.onWheel(0.5, 0.5, 100, 0, RECTS)
    synth.end(true)
    const cancels = all().filter((s) => s.action === 3)
    expect(cancels.map((s) => s.pointerId)).toEqual([240, 241])
    const total = all().length
    synth.end(true)
    expect(all()).toHaveLength(total)
    expect(() => vi.advanceTimersByTime(100)).not.toThrow()
    expect(all()).toHaveLength(total)
    void TOUCH_ACTION_UP
  })
})
