// Synthesized two-finger pinch: Ctrl+wheel / trackpad pinch (browsers report
// both as wheel+ctrlKey) is orchestrated into one continuous two-finger
// gesture on the phone. Ported from plain-cast's gesture stack.

import {
  TOUCH_ACTION_CANCEL,
  TOUCH_ACTION_DOWN,
  TOUCH_ACTION_MOVE,
  TOUCH_ACTION_UP,
  type TouchSample,
} from './touch-frame'
import { createTouchIndicator, hideIndicator, showIndicator } from './touch-indicator'
import type { RectPair } from './view-rects'

// The frame protocol's pointerId is one u8; real DOM pointerIds are small
// (mouse is always 1), so the two virtual fingers take the high range.
const PINCH_ID_A = 240
const PINCH_ID_B = 241
/** Initial half-distance between the two fingers (canvas CSS pixels). */
const PINCH_BASE_RADIUS_PX = 60
const PINCH_MIN_RADIUS = 0.02
const PINCH_MAX_RADIUS = 0.38
/** Exponential rate: one wheel notch deltaY≈100 → half-distance ×1.22; a
 *  trackpad pinch's continuous small deltas accumulate under the same formula. */
const PINCH_WHEEL_RATE = 0.002
/** deltaMode=LINE (Firefox): one notch deltaY≈3, converted to pixel equivalent. */
const PINCH_LINE_TO_PX = 33
/** After this much idle wheel time with the radius converged, lift the
 *  fingers; continued scrolling keeps them down. */
const PINCH_IDLE_MS = 140
/** Convergence step period (frame-rate aligned, 60Hz). */
const PINCH_TICK_MS = 16

interface PinchState {
  /** Finger anchor (normalized), fingers spread horizontally at ax±radius. */
  ax: number
  ay: number
  /** Current half-distance (normalized), stepped toward target each tick. */
  radius: number
  target: number
  lastWheelAt: number
  settled: boolean
  dots: [HTMLDivElement, HTMLDivElement]
  rects: RectPair
  timer: number
}

/**
 * Pinch state machine: the first event plants both fingers (DOWN×2),
 * continued scrolling only retargets the radius while a timer steps it
 * toward the target (spread = zoom in, close = zoom out), and idle +
 * converged input lifts them (UP×2) — the whole sequence is one continuous
 * real pinch, not a series of jittery open/close gestures.
 *
 * The virtual fingers are independent of the real pointer stream (they don't
 * share the DOM pointer table); mutual exclusion with real gestures is the
 * owner's job: ScreenMirrorControl ends the pinch before a real pointerdown.
 */
export class PinchSynthesizer {
  private state: PinchState | null = null

  constructor(
    private overlay: HTMLElement,
    private sendSamples: (samples: TouchSample[]) => void,
  ) {}

  get active(): boolean {
    return this.state !== null
  }

  /** One Ctrl+wheel delta; [rects] is the render rect pair at call time. */
  onWheel(nx: number, ny: number, deltaY: number, deltaMode: number, rects: RectPair) {
    const px = deltaY * (deltaMode === 0 ? 1 : PINCH_LINE_TO_PX)
    if (!this.state) this.start(nx, ny, rects)
    const p = this.state
    if (!p) return
    p.rects = rects
    p.lastWheelAt = performance.now()
    // Clamp the per-event factor: Safari pinch momentum can emit an occasional
    // huge delta that shouldn't jump the full range in one step
    const factor = Math.min(1.6, Math.max(0.6, Math.exp(-px * PINCH_WHEEL_RATE)))
    p.target = Math.min(PINCH_MAX_RADIUS, Math.max(PINCH_MIN_RADIUS, p.target * factor))
  }

  private start(nx: number, ny: number, rects: RectPair) {
    let r = PINCH_BASE_RADIUS_PX / rects.canvasRect.width
    if (!Number.isFinite(r)) return
    r = Math.min(PINCH_MAX_RADIUS, 0.3, Math.max(PINCH_MIN_RADIUS, r))
    // Pull the anchor in so both fingers land inside the picture
    const ax = Math.min(1 - r, Math.max(r, nx))
    const ay = Math.min(0.98, Math.max(0.02, ny))
    const dots: [HTMLDivElement, HTMLDivElement] = [
      createTouchIndicator(this.overlay),
      createTouchIndicator(this.overlay),
    ]
    this.state = {
      ax,
      ay,
      radius: r,
      target: r,
      lastWheelAt: performance.now(),
      settled: true,
      dots,
      rects,
      timer: window.setInterval(() => this.tick(), PINCH_TICK_MS),
    }
    this.sendSamples([
      { action: TOUCH_ACTION_DOWN, pointerId: PINCH_ID_A, x: ax - r, y: ay, dtMs: 0 },
      { action: TOUCH_ACTION_DOWN, pointerId: PINCH_ID_B, x: ax + r, y: ay, dtMs: 0 },
    ])
    this.placeDots()
  }

  private tick() {
    const p = this.state
    if (!p) return
    const gap = p.target - p.radius
    if (Math.abs(gap) > 0.0015) {
      p.radius += gap * 0.4
      p.settled = false
      this.sendSamples([
        { action: TOUCH_ACTION_MOVE, pointerId: PINCH_ID_A, x: p.ax - p.radius, y: p.ay, dtMs: PINCH_TICK_MS },
        { action: TOUCH_ACTION_MOVE, pointerId: PINCH_ID_B, x: p.ax + p.radius, y: p.ay, dtMs: PINCH_TICK_MS },
      ])
      this.placeDots()
    } else {
      p.settled = true
    }
    if (p.settled && performance.now() - p.lastWheelAt > PINCH_IDLE_MS) this.end(false)
  }

  /** Indicator placement: normalized canvas coords → overlay-local pixels. */
  private placeDots() {
    const p = this.state
    if (!p) return
    const c = p.rects.canvasRect
    const o = p.rects.overlayRect
    const offX = c.left - o.left
    const offY = c.top - o.top
    showIndicator(p.dots[0], offX + (p.ax - p.radius) * c.width, offY + p.ay * c.height)
    showIndicator(p.dots[1], offX + (p.ax + p.radius) * c.width, offY + p.ay * c.height)
  }

  /** End the gesture: lift the fingers (or cancel) and clean up. Idempotent. */
  end(cancelled: boolean) {
    const p = this.state
    if (!p) return
    this.state = null
    window.clearInterval(p.timer)
    const action = cancelled ? TOUCH_ACTION_CANCEL : TOUCH_ACTION_UP
    this.sendSamples([
      { action, pointerId: PINCH_ID_A, x: p.ax - p.radius, y: p.ay, dtMs: 0 },
      { action, pointerId: PINCH_ID_B, x: p.ax + p.radius, y: p.ay, dtMs: 0 },
    ])
    for (const dot of p.dots) hideIndicator(dot, false, true)
  }
}
