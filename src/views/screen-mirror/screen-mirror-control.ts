import { ref, onUnmounted, type Ref } from 'vue'
import { gqlFetch } from '@/lib/api/gql-client'
import { sendScreenMirrorControlGQL } from '@/lib/api/mutation'

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

function normalizeCoords(
  clientX: number,
  clientY: number,
  overlayEl: HTMLElement,
  canvasEl: HTMLCanvasElement
): { x: number; y: number } | null {
  const overlayRect = overlayEl.getBoundingClientRect()
  const videoWidth = canvasEl.width
  const videoHeight = canvasEl.height
  if (!videoWidth || !videoHeight) return null

  const containerW = overlayRect.width
  const containerH = overlayRect.height
  if (!containerW || !containerH) return null
  const containerAspect = containerW / containerH
  const videoAspect = videoWidth / videoHeight

  let renderW: number, renderH: number, offsetX: number, offsetY: number

  if (videoAspect > containerAspect) {
    renderW = containerW
    renderH = containerW / videoAspect
    offsetX = 0
    offsetY = (containerH - renderH) / 2
  } else {
    renderH = containerH
    renderW = containerH * videoAspect
    offsetX = (containerW - renderW) / 2
    offsetY = 0
  }

  const localX = clientX - overlayRect.left - offsetX
  const localY = clientY - overlayRect.top - offsetY

  if (localX < 0 || localX > renderW || localY < 0 || localY > renderH) {
    return null
  }

  return {
    x: Math.max(0, Math.min(1, localX / renderW)),
    y: Math.max(0, Math.min(1, localY / renderH)),
  }
}

const SAMPLE_INTERVAL = 10
const TAP_MOVE_THRESHOLD_NORM = 0.01
const TAP_MAX_MS = 300
const LONG_PRESS_MS = 450

interface ActiveGesture {
  pointerId: number
  downX: number
  downY: number
  downTime: number
  lastNormX: number
  lastNormY: number
  lastSampleTime: number
  longPressTimer: ReturnType<typeof setTimeout> | null
  longPressFired: boolean
  streamStarted: boolean
}

function createTouchIndicator(container: HTMLElement): HTMLElement {
  const dot = document.createElement('div')
  dot.className = 'touch-indicator'
  container.appendChild(dot)
  return dot
}

function positionIndicator(dot: HTMLElement, x: number, y: number) {
  dot.style.left = `${x}px`
  dot.style.top = `${y}px`
}

function showIndicator(dot: HTMLElement, x: number, y: number) {
  positionIndicator(dot, x, y)
  dot.classList.remove('touch-indicator--fade-out')
  dot.classList.add('touch-indicator--active')
}

function hideIndicator(dot: HTMLElement, isTap: boolean) {
  if (isTap) {
    dot.classList.add('touch-indicator--ripple')
  }
  dot.classList.remove('touch-indicator--active')
  dot.classList.add('touch-indicator--fade-out')
  dot.addEventListener('transitionend', () => {
    dot.classList.remove('touch-indicator--fade-out', 'touch-indicator--ripple', 'touch-indicator--dragging', 'touch-indicator--long-press')
  }, { once: true })
}

export function useScreenMirrorControl(
  canvasRef: Ref<HTMLCanvasElement | undefined>,
  enabled: Ref<boolean>
) {
  const overlayRef = ref<HTMLDivElement>()
  let gesture: ActiveGesture | null = null
  let touchDot: HTMLElement | null = null

  let chain: Promise<unknown> = Promise.resolve()
  let moveInFlight = false
  let pendingMove: { x: number; y: number; pointerId: number } | null = null

  const send = (event: ScreenMirrorControlEvent) => {
    chain = chain.then(() =>
      gqlFetch(sendScreenMirrorControlGQL, { input: event }).catch((err) => {
        console.error('Screen mirror control error:', event.action, err)
      })
    )
  }

  const sendMove = (x: number, y: number, pointerId: number) => {
    if (moveInFlight) {
      pendingMove = { x, y, pointerId }
      return
    }
    moveInFlight = true
    const dispatch = (cx: number, cy: number, cp: number) => {
      chain = chain
        .then(() =>
          gqlFetch(sendScreenMirrorControlGQL, {
            input: { action: 'TOUCH_MOVE', x: cx, y: cy, pointerId: cp, pressure: 1 },
          }).catch((err) => {
            console.error('TOUCH_MOVE error:', err)
          })
        )
        .then(() => {
          const next = pendingMove
          pendingMove = null
          if (next) {
            dispatch(next.x, next.y, next.pointerId)
          } else {
            moveInFlight = false
          }
        })
    }
    dispatch(x, y, pointerId)
  }

  const flushStream = () => {
    pendingMove = null
    moveInFlight = false
    chain = Promise.resolve()
  }

  const localPos = (clientX: number, clientY: number): { lx: number; ly: number } | null => {
    const el = overlayRef.value
    if (!el) return null
    const rect = el.getBoundingClientRect()
    return { lx: clientX - rect.left, ly: clientY - rect.top }
  }

  const onPointerDown = (e: PointerEvent) => {
    if (!enabled.value) return
    const canvas = canvasRef.value
    const overlay = overlayRef.value
    if (!canvas || !overlay) return

    const coords = normalizeCoords(e.clientX, e.clientY, overlay, canvas)
    if (!coords) return

    e.preventDefault()
    const target = e.target as HTMLElement
    target.setPointerCapture(e.pointerId)
    target.style.touchAction = 'none'
    target.style.userSelect = 'none'

    flushStream()

    const now = performance.now()
    gesture = {
      pointerId: e.pointerId,
      downX: coords.x,
      downY: coords.y,
      downTime: now,
      lastNormX: coords.x,
      lastNormY: coords.y,
      lastSampleTime: now,
      longPressTimer: setTimeout(() => {
        if (gesture && !gesture.streamStarted) {
          gesture.longPressFired = true
          send({
            action: 'LONG_PRESS',
            x: gesture.downX,
            y: gesture.downY,
            durationMs: 520,
          })
          if (touchDot) touchDot.classList.add('touch-indicator--long-press')
        }
      }, LONG_PRESS_MS),
      longPressFired: false,
      streamStarted: false,
    }

    const pos = localPos(e.clientX, e.clientY)
    if (pos && touchDot) {
      showIndicator(touchDot, pos.lx, pos.ly)
    }
  }

  const onPointerMove = (e: PointerEvent) => {
    if (!gesture || !enabled.value || e.pointerId !== gesture.pointerId) return
    const canvas = canvasRef.value
    const overlay = overlayRef.value
    if (!canvas || !overlay) return

    const now = performance.now()
    if (now - gesture.lastSampleTime < SAMPLE_INTERVAL) return

    const coords = normalizeCoords(e.clientX, e.clientY, overlay, canvas)
    if (!coords) return

    gesture.lastSampleTime = now

    const dx = coords.x - gesture.downX
    const dy = coords.y - gesture.downY
    const distFromDown = Math.sqrt(dx * dx + dy * dy)

    if (!gesture.streamStarted && distFromDown > TAP_MOVE_THRESHOLD_NORM) {
      gesture.streamStarted = true
      if (gesture.longPressTimer) {
        clearTimeout(gesture.longPressTimer)
        gesture.longPressTimer = null
      }
      send({
        action: 'TOUCH_DOWN',
        x: gesture.downX,
        y: gesture.downY,
        pointerId: gesture.pointerId,
        pressure: 1,
      })
      if (touchDot) touchDot.classList.add('touch-indicator--dragging')
    }

    if (gesture.streamStarted) {
      gesture.lastNormX = coords.x
      gesture.lastNormY = coords.y
      sendMove(coords.x, coords.y, gesture.pointerId)
    }

    const pos = localPos(e.clientX, e.clientY)
    if (pos && touchDot) {
      positionIndicator(touchDot, pos.lx, pos.ly)
    }
  }

  const finalizeGesture = (e: PointerEvent | null, isCancel: boolean) => {
    if (!gesture) return
    const g = gesture
    if (g.longPressTimer) {
      clearTimeout(g.longPressTimer)
      g.longPressTimer = null
    }

    const canvas = canvasRef.value
    const overlay = overlayRef.value
    let finalX = g.lastNormX
    let finalY = g.lastNormY

    if (!isCancel && canvas && overlay && e) {
      const coords = normalizeCoords(e.clientX, e.clientY, overlay, canvas)
      if (coords) {
        finalX = coords.x
        finalY = coords.y
      }
    }

    let isTap = false
    if (!isCancel && !g.streamStarted && !g.longPressFired) {
      const now = performance.now()
      const dx = finalX - g.downX
      const dy = finalY - g.downY
      const dist = Math.sqrt(dx * dx + dy * dy)
      const elapsed = now - g.downTime
      if (dist < TAP_MOVE_THRESHOLD_NORM && elapsed < TAP_MAX_MS) {
        isTap = true
      }
    }

    if (g.streamStarted) {
      pendingMove = null
      send({
        action: 'TOUCH_UP',
        x: finalX,
        y: finalY,
        pointerId: g.pointerId,
        pressure: 0,
      })
    } else if (isTap) {
      send({
        action: 'TAP',
        x: g.downX,
        y: g.downY,
      })
    }

    if (touchDot) {
      touchDot.classList.remove('touch-indicator--dragging', 'touch-indicator--long-press')
      hideIndicator(touchDot, isTap)
    }

    gesture = null
  }

  const onPointerUp = (e: PointerEvent) => {
    if (!gesture || !enabled.value) return
    finalizeGesture(e, false)
  }

  const onPointerCancel = () => {
    if (!gesture) return
    finalizeGesture(null, true)
  }

  const onWheel = (e: WheelEvent) => {
    if (!enabled.value) return
    const canvas = canvasRef.value
    const overlay = overlayRef.value
    if (!canvas || !overlay) return

    const coords = normalizeCoords(e.clientX, e.clientY, overlay, canvas)
    if (!coords) return

    e.preventDefault()

    send({
      action: 'SCROLL',
      x: coords.x,
      y: coords.y,
      deltaX: e.deltaX,
      deltaY: e.deltaY,
    })
  }

  const onKeyDown = (e: KeyboardEvent) => {
    if (!enabled.value) return

    let handled = true
    switch (e.key) {
      case 'Escape':
      case 'Backspace':
        send({ action: 'BACK' })
        break
      case 'Home':
        send({ action: 'HOME' })
        break
      default:
        handled = false
    }

    if (handled) {
      e.preventDefault()
      e.stopPropagation()
    }
  }

  const attachOverlay = (el: HTMLDivElement | undefined) => {
    if (touchDot && touchDot.parentElement) {
      touchDot.parentElement.removeChild(touchDot)
      touchDot = null
    }
    overlayRef.value = el
    if (el) {
      el.style.touchAction = 'none'
      el.style.userSelect = 'none'
      touchDot = createTouchIndicator(el)
    }
  }

  const setupListeners = () => {
    const el = overlayRef.value
    if (!el) return

    el.addEventListener('pointerdown', onPointerDown)
    el.addEventListener('pointermove', onPointerMove)
    el.addEventListener('pointerup', onPointerUp)
    el.addEventListener('pointercancel', onPointerCancel)
    el.addEventListener('wheel', onWheel, { passive: false })
    el.addEventListener('keydown', onKeyDown)
  }

  const removeListeners = () => {
    const el = overlayRef.value
    if (!el) return

    el.removeEventListener('pointerdown', onPointerDown)
    el.removeEventListener('pointermove', onPointerMove)
    el.removeEventListener('pointerup', onPointerUp)
    el.removeEventListener('pointercancel', onPointerCancel)
    el.removeEventListener('wheel', onWheel)
    el.removeEventListener('keydown', onKeyDown)
  }

  onUnmounted(() => {
    if (gesture) {
      const g = gesture
      if (g.longPressTimer) clearTimeout(g.longPressTimer)
      if (g.streamStarted) {
        pendingMove = null
        send({
          action: 'TOUCH_UP',
          x: g.lastNormX,
          y: g.lastNormY,
          pointerId: g.pointerId,
          pressure: 0,
        })
      }
      gesture = null
    }
    removeListeners()
    if (touchDot && touchDot.parentElement) {
      touchDot.parentElement.removeChild(touchDot)
      touchDot = null
    }
  })

  return {
    overlayRef,
    attachOverlay,
    setupListeners,
    removeListeners,
    sendControl: send,
  }
}
