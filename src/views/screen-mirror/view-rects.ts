import { normalizeCoords } from './touch-coords'

/** The canvas and overlay render rects, refreshed as a pair. */
export interface RectPair {
  canvasRect: DOMRect
  overlayRect: DOMRect
}

/**
 * canvas/overlay render-rect cache.
 *
 * Refresh() once at gesture start (DOWN/SCROLL/pinch): the layout can move
 * without any resize/scroll event firing, and a stale cache shifts every
 * subsequent injected coordinate. The hot path (MOVE) only reads the cache —
 * a getBoundingClientRect per move event would force layout hundreds of
 * times per second while dragging. resize/visualViewport changes invalidate;
 * the next gesture start naturally re-reads.
 */
export class ViewRects {
  private pair: RectPair | null = null

  /** Unconditionally re-read and cache; returns the fresh pair. */
  refresh(canvas: HTMLCanvasElement, overlay: HTMLElement): RectPair {
    this.pair = {
      canvasRect: canvas.getBoundingClientRect(),
      overlayRect: overlay.getBoundingClientRect(),
    }
    return this.pair
  }

  invalidate() {
    this.pair = null
  }

  /** Client coords → normalized 0..1 video coords (clamped; null when no picture). */
  normalized(clientX: number, clientY: number, canvas: HTMLCanvasElement): { x: number; y: number } | null {
    const pair = this.pair
    if (!pair) return null
    return normalizeCoords(clientX, clientY, pair.canvasRect, canvas)
  }

  /** Client coords → overlay-local pixels (touch-indicator positioning). */
  local(clientX: number, clientY: number): { lx: number; ly: number } | null {
    const pair = this.pair
    if (!pair) return null
    return { lx: clientX - pair.overlayRect.left, ly: clientY - pair.overlayRect.top }
  }
}
