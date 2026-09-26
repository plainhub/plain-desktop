// Screen mirror gesture control, ported from plain-cast's low-latency design
// (docs/touch-low-latency-design.md): pointerdown injects the contact
// immediately (no tap/drag classification delay), pointermove emits every
// coalesced sample at digitizer rate, a tap is a natural short down+up, a
// long-press is a natural hold (server-side keep-alive). Touch samples ride
// one compact binary WS frame; cold-path actions (BACK/HOME/SCROLL/…) go as
// encrypted JSON on the same socket with a GraphQL fallback.
import { onUnmounted, watch, type Ref } from 'vue'
import emitter from '@/plugins/eventbus'
import { gqlFetch } from '@/lib/api/gql-client'
import { sendScreenMirrorControlGQL } from '@/lib/api/mutation'
import { sendAppWsBytes, sendAppWsJson } from '@/hooks/app-socket'
import { PinchSynthesizer } from './pinch-synthesizer'
import {
  createTouchIndicator,
  hideIndicator,
  positionIndicator,
  showIndicator,
} from './touch-indicator'
import {
  TOUCH_ACTION_CANCEL,
  TOUCH_ACTION_DOWN,
  TOUCH_ACTION_MOVE,
  TOUCH_ACTION_UP,
  encodeTouchFrame,
  type TouchSample,
} from './touch-frame'
import { ViewRects } from './view-rects'

export interface TouchPoint {
  x: number
  y: number
  tMs: number
}

export interface ScreenMirrorControlEvent {
  action: ScreenMirrorControlAction
  x?: number
  y?: number
  endX?: number
  endY?: number
  durationMs?: number
  deltaX?: number
  deltaY?: number
  key?: string
  pathPoints?: TouchPoint[]
  pointerId?: number
  pressure?: number
}

export type ScreenMirrorControlAction =
  | 'TAP'
  | 'LONG_PRESS'
  | 'SWIPE'
  | 'SCROLL'
  | 'BACK'
  | 'HOME'
  | 'RECENTS'
  | 'LOCK_SCREEN'
  | 'KEY'
  | 'TOUCH'
  | 'TOUCH_DOWN'
  | 'TOUCH_MOVE'
  | 'TOUCH_UP'

const TAP_MOVE_THRESHOLD_NORM = 0.01
const TAP_MAX_MS = 300

interface ActivePointer {
  pointerId: number
  downX: number
  downY: number
  downTime: number
  /** timeStamp (DOMHighRes) of the last emitted sample. */
  lastSampleTs: number
  /** Last known normalized coordinates (for recovering a lost pointerup). */
  lastX: number
  lastY: number
  movedBeyondTap: boolean
  dot: HTMLDivElement
}

/**
 * Gesture surface owner: real DOM pointers → a touch sample stream (tap
 * detection, dead-pointer recovery), plus wheel → JSON control events.
 * Keyboard stays on the overlay (Escape/Backspace/Home below). Coordinate
 * mapping and caching are delegated to ViewRects, Ctrl+wheel pinch to
 * PinchSynthesizer, feedback dots to touch-indicator.
 */
export class ScreenMirrorControl {
  private canvas: HTMLCanvasElement
  private overlay: HTMLElement
  private sendControlFn: (event: ScreenMirrorControlEvent) => void
  private sendSamplesFn: (samples: TouchSample[]) => void
  private enabled = false
  private pointers = new Map<number, ActivePointer>()
  /** Render-rect cache (refresh at gesture start, hot-path reads). */
  private rects = new ViewRects()
  /** Synthesized two-finger pinch; mutually exclusive with the real pointer
   *  stream (collapsed before DOWN, see onPointerDown). */
  private pinch: PinchSynthesizer

  private boundPointerDown: (e: PointerEvent) => void
  private boundPointerMove: (e: PointerEvent) => void
  private boundPointerUp: (e: PointerEvent) => void
  private boundPointerCancel: (e: PointerEvent) => void
  private boundLostPointercapture: (e: PointerEvent) => void
  private boundWheel: (e: WheelEvent) => void
  private boundContextMenu: (e: MouseEvent) => void
  private boundInvalidateRect: () => void

  /** Mouse navigation: right button = BACK, middle button = HOME. */
  middleClickHome = true

  constructor(
    canvas: HTMLCanvasElement,
    overlay: HTMLElement,
    sendControl: (event: ScreenMirrorControlEvent) => void,
    sendSamples: (samples: TouchSample[]) => void,
  ) {
    this.canvas = canvas
    this.overlay = overlay
    this.sendControlFn = sendControl
    this.sendSamplesFn = sendSamples
    this.pinch = new PinchSynthesizer(overlay, sendSamples)
    this.boundPointerDown = (e) => this.onPointerDown(e)
    this.boundPointerMove = (e) => this.onPointerMove(e)
    this.boundPointerUp = (e) => this.onPointerUp(e)
    this.boundPointerCancel = (e) => this.onPointerCancel(e)
    this.boundLostPointercapture = (e) => this.onLostPointercapture(e)
    this.boundWheel = (e) => this.onWheel(e)
    this.boundContextMenu = (e) => e.preventDefault()
    this.boundInvalidateRect = () => this.rects.invalidate()
  }

  setEnabled(on: boolean) {
    if (this.enabled === on) return
    this.enabled = on
    // Mid-gesture shutdown: finish with CANCEL. A started gesture must have a
    // terminal event, otherwise the server's pointer table keeps a dead
    // pointer and every later tap on that screen becomes a malformed
    // multi-pointer event.
    if (!on) {
      this.pinch.end(true)
      this.cancelActivePointers()
    }
  }

  /** Cancel all in-flight gestures (control disabled / surface destroyed). */
  private cancelActivePointers() {
    for (const p of [...this.pointers.values()]) {
      this.sendSamplesFn([
        { action: TOUCH_ACTION_CANCEL, pointerId: p.pointerId, x: p.lastX, y: p.lastY, dtMs: 0 },
      ])
      this.removePointer(p, false)
    }
  }

  /**
   * Mount: touch styles and all listeners in one go.
   * touchAction='none' is required for multi-pointer: the browser must not
   * steal the second finger for page pinch-zoom — pinches belong to the phone.
   */
  setupListeners() {
    this.overlay.style.touchAction = 'none'
    this.overlay.style.userSelect = 'none'
    this.overlay.addEventListener('pointerdown', this.boundPointerDown)
    this.overlay.addEventListener('pointermove', this.boundPointerMove)
    this.overlay.addEventListener('pointerup', this.boundPointerUp)
    this.overlay.addEventListener('pointercancel', this.boundPointerCancel)
    this.overlay.addEventListener('lostpointercapture', this.boundLostPointercapture)
    this.overlay.addEventListener('wheel', this.boundWheel, { passive: false })
    this.overlay.addEventListener('contextmenu', this.boundContextMenu)
    window.addEventListener('resize', this.boundInvalidateRect)
    window.visualViewport?.addEventListener('resize', this.boundInvalidateRect)
    window.visualViewport?.addEventListener('scroll', this.boundInvalidateRect)
  }

  removeListeners() {
    this.overlay.removeEventListener('pointerdown', this.boundPointerDown)
    this.overlay.removeEventListener('pointermove', this.boundPointerMove)
    this.overlay.removeEventListener('pointerup', this.boundPointerUp)
    this.overlay.removeEventListener('pointercancel', this.boundPointerCancel)
    this.overlay.removeEventListener('lostpointercapture', this.boundLostPointercapture)
    this.overlay.removeEventListener('wheel', this.boundWheel)
    this.overlay.removeEventListener('contextmenu', this.boundContextMenu)
    window.removeEventListener('resize', this.boundInvalidateRect)
    window.visualViewport?.removeEventListener('resize', this.boundInvalidateRect)
    window.visualViewport?.removeEventListener('scroll', this.boundInvalidateRect)
  }

  private send(event: ScreenMirrorControlEvent) {
    this.sendControlFn(event)
  }

  /** dt since the previous sample of this pointer stream (0 for the first). */
  private sampleDt(p: ActivePointer, timeStamp: number): number {
    const dt = timeStamp - p.lastSampleTs
    p.lastSampleTs = timeStamp
    return dt > 0 ? dt : 0
  }

  private removePointer(p: ActivePointer, isTap: boolean) {
    this.pointers.delete(p.pointerId)
    hideIndicator(p.dot, isTap, true)
  }

  /** Release a pointer whose pointerup was lost — otherwise the phone keeps
   *  holding the finger down and a quick tap turns into a long-press there. */
  private releaseStalePointer(p: ActivePointer) {
    this.sendSamplesFn([
      { action: TOUCH_ACTION_UP, pointerId: p.pointerId, x: p.lastX, y: p.lastY, dtMs: 0 },
    ])
    this.removePointer(p, false)
  }

  private onPointerDown(e: PointerEvent) {
    if (!this.enabled) return
    // Mouse navigation buttons: right = BACK, middle = HOME. contextmenu is
    // suppressed globally on the overlay.
    if (e.button === 2) {
      e.preventDefault()
      this.send({ action: 'BACK' })
      return
    }
    if (e.button === 1 && this.middleClickHome) {
      e.preventDefault()
      this.send({ action: 'HOME' })
      return
    }
    // Gesture start unconditionally refreshes the rect cache (the layout can
    // move without any resize/scroll event, see ViewRects)
    this.rects.refresh(this.canvas, this.overlay)
    const coords = this.rects.normalized(e.clientX, e.clientY, this.canvas)
    if (!coords) return
    e.preventDefault()
    // Collapse the virtual pinch before a real finger lands, so the two
    // never merge into a three-finger gesture
    if (this.pinch.active) this.pinch.end(false)
    const target = e.target as HTMLElement
    target.setPointerCapture(e.pointerId)
    target.style.touchAction = 'none'
    target.style.userSelect = 'none'
    // Same pointerId still tracked → its pointerup was lost; release it first
    const stale = this.pointers.get(e.pointerId)
    if (stale) this.releaseStalePointer(stale)
    const p: ActivePointer = {
      pointerId: e.pointerId,
      downX: coords.x,
      downY: coords.y,
      downTime: performance.now(),
      lastSampleTs: e.timeStamp,
      lastX: coords.x,
      lastY: coords.y,
      movedBeyondTap: false,
      dot: createTouchIndicator(this.overlay),
    }
    this.pointers.set(e.pointerId, p)
    // Contact is injected immediately — no tap/drag classification delay
    this.sendSamplesFn([
      { action: TOUCH_ACTION_DOWN, pointerId: e.pointerId, x: coords.x, y: coords.y, dtMs: 0 },
    ])
    const pos = this.rects.local(e.clientX, e.clientY)
    if (pos) showIndicator(p.dot, pos.lx, pos.ly)
  }

  private onPointerMove(e: PointerEvent) {
    const p = this.pointers.get(e.pointerId)
    if (!p || !this.enabled) return
    // Emit every raw sample (up to the digitizer rate), not just the latest.
    // getCoalescedEvents is missing on some WebKit builds — fall back to [e]
    const coalesced =
      typeof e.getCoalescedEvents === 'function' ? e.getCoalescedEvents() : []
    const events = coalesced.length > 0 ? coalesced : [e]
    const samples: TouchSample[] = []
    for (const ev of events) {
      const coords = this.rects.normalized(ev.clientX, ev.clientY, this.canvas)
      if (!coords) continue
      p.lastX = coords.x
      p.lastY = coords.y
      samples.push({
        action: TOUCH_ACTION_MOVE,
        pointerId: p.pointerId,
        x: coords.x,
        y: coords.y,
        dtMs: this.sampleDt(p, ev.timeStamp),
      })
      if (!p.movedBeyondTap) {
        const dx = coords.x - p.downX
        const dy = coords.y - p.downY
        if (Math.sqrt(dx * dx + dy * dy) > TAP_MOVE_THRESHOLD_NORM) {
          p.movedBeyondTap = true
          p.dot.classList.add('touch-indicator--dragging')
        }
      }
    }
    if (samples.length > 0) this.sendSamplesFn(samples)
    const pos = this.rects.local(e.clientX, e.clientY)
    if (pos) positionIndicator(p.dot, pos.lx, pos.ly)
  }

  private onPointerUp(e: PointerEvent) {
    const p = this.pointers.get(e.pointerId)
    // UP is not gated on enabled (same for cancel/lostpointercapture below):
    // enabled only blocks new DOWN gestures. If the UP of an already-sent
    // DOWN is swallowed, the server keeps a dead pointer and touch on the
    // screen dies from then on.
    if (!p) return
    const coords = this.rects.normalized(e.clientX, e.clientY, this.canvas)
    const end = coords ?? { x: p.lastX, y: p.lastY }
    // A tap is a natural short down+up on the phone — nothing to classify
    this.sendSamplesFn([
      {
        action: TOUCH_ACTION_UP,
        pointerId: p.pointerId,
        x: end.x,
        y: end.y,
        dtMs: this.sampleDt(p, e.timeStamp),
      },
    ])
    const elapsed = performance.now() - p.downTime
    const dx = end.x - p.downX
    const dy = end.y - p.downY
    const isTap =
      !p.movedBeyondTap && elapsed < TAP_MAX_MS &&
      Math.sqrt(dx * dx + dy * dy) < TAP_MOVE_THRESHOLD_NORM
    p.dot.classList.remove('touch-indicator--dragging', 'touch-indicator--long-press')
    this.removePointer(p, isTap)
  }

  private onPointerCancel(e: PointerEvent) {
    const p = this.pointers.get(e.pointerId)
    if (!p) return
    const coords = this.rects.normalized(e.clientX, e.clientY, this.canvas)
    const end = coords ?? { x: p.lastX, y: p.lastY }
    this.sendSamplesFn([
      {
        action: TOUCH_ACTION_CANCEL,
        pointerId: p.pointerId,
        x: end.x,
        y: end.y,
        dtMs: this.sampleDt(p, e.timeStamp),
      },
    ])
    p.dot.classList.remove('touch-indicator--dragging', 'touch-indicator--long-press')
    this.removePointer(p, false)
  }

  /** Capture release without a preceding pointerup → the up was lost;
   *  synthesize it so the phone-side touch ends immediately. */
  private onLostPointercapture(e: PointerEvent) {
    const p = this.pointers.get(e.pointerId)
    if (!p) return
    this.releaseStalePointer(p)
  }

  private onWheel(e: WheelEvent) {
    if (!this.enabled) return
    // Same as onPointerDown: refresh unconditionally before scrolling, to
    // avoid coordinate drift from layout moves
    const fresh = this.rects.refresh(this.canvas, this.overlay)
    const coords = this.rects.normalized(e.clientX, e.clientY, this.canvas)
    if (!coords) return
    e.preventDefault()
    if (e.ctrlKey) {
      // Trackpad pinch / Ctrl+wheel: browsers report both as wheel+ctrlKey;
      // convert to a two-finger pinch on the phone (spread = zoom in,
      // close = zoom out); a plain wheel keeps its pan semantics.
      this.pinch.onWheel(coords.x, coords.y, e.deltaY, e.deltaMode, fresh)
      return
    }
    this.send({
      action: 'SCROLL',
      x: coords.x,
      y: coords.y,
      deltaX: e.deltaX,
      deltaY: e.deltaY,
    })
  }

  destroy() {
    this.removeListeners()
    // In-flight gestures get a final CANCEL: after destruction the listeners
    // are gone, no real pointerup will be processed anymore. When the WS is
    // already dead the sends drop silently; the server's disconnect reset is
    // the backstop.
    this.pinch.end(true)
    this.cancelActivePointers()
  }
}

export function useScreenMirrorControl(
  canvasRef: Ref<HTMLCanvasElement | undefined>,
  enabled: Ref<boolean>,
) {
  let control: ScreenMirrorControl | null = null
  let overlayEl: HTMLElement | null = null

  const sendControl = (event: ScreenMirrorControlEvent) => {
    if (sendAppWsJson(event)) return
    gqlFetch(sendScreenMirrorControlGQL, { input: event }).catch((err) => {
      console.error('Screen mirror control error:', event.action, err)
    })
  }

  const sendSamples = (samples: TouchSample[]) => {
    sendAppWsBytes(encodeTouchFrame(samples))
  }

  const destroyControl = () => {
    overlayEl?.removeEventListener('keydown', onKeyDown)
    control?.destroy()
    control = null
  }

  const attachOverlay = (el: HTMLDivElement | undefined) => {
    destroyControl()
    overlayEl = el ?? null
  }

  const setupListeners = () => {
    const canvas = canvasRef.value
    if (!canvas || !overlayEl) return
    destroyControl()
    overlayEl.addEventListener('keydown', onKeyDown)
    control = new ScreenMirrorControl(canvas, overlayEl, sendControl, sendSamples)
    control.setEnabled(enabled.value)
    control.setupListeners()
  }

  const removeListeners = () => destroyControl()

  const onKeyDown = (e: KeyboardEvent) => {
    if (!enabled.value) return

    let handled = true
    switch (e.key) {
      case 'Escape':
      case 'Backspace':
        sendControl({ action: 'BACK' })
        break
      case 'Home':
        sendControl({ action: 'HOME' })
        break
      default:
        handled = false
    }

    if (handled) {
      e.preventDefault()
      e.stopPropagation()
    }
  }

  const onConnectionChanged = (up: boolean) => {
    // A dropped app socket kills the sample transport mid-gesture: cancel
    // locally (the phone resets its injector on disconnect); re-arm on the
    // next successful dial.
    if (up) {
      control?.setEnabled(enabled.value)
    } else {
      control?.setEnabled(false)
    }
  }

  watch(enabled, (v) => control?.setEnabled(v))
  emitter.on('app_socket_connection_changed', onConnectionChanged)

  onUnmounted(() => {
    emitter.off('app_socket_connection_changed', onConnectionChanged)
    destroyControl()
  })

  return {
    attachOverlay,
    setupListeners,
    removeListeners,
    sendControl,
  }
}
