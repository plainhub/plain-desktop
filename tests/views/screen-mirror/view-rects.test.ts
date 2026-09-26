import { describe, expect, it, vi } from 'vitest'
import { ViewRects } from '@/views/screen-mirror/view-rects'

function makeElements() {
  const canvas = document.createElement('canvas')
  canvas.width = 400
  canvas.height = 800
  const overlay = document.createElement('div')
  const canvasSpy = vi.spyOn(canvas, 'getBoundingClientRect')
    .mockReturnValue(new DOMRect(0, 100, 400, 800))
  const overlaySpy = vi.spyOn(overlay, 'getBoundingClientRect')
    .mockReturnValue(new DOMRect(0, 0, 400, 900))
  return { canvas, overlay, canvasSpy, overlaySpy }
}

describe('ViewRects', () => {
  it('returns null before the first refresh (no picture mapped yet)', () => {
    const { canvas, overlay } = makeElements()
    const rects = new ViewRects()
    expect(rects.normalized(0, 0, canvas)).toBeNull()
    expect(rects.local(0, 0)).toBeNull()
    void overlay
  })

  it('refresh caches the pair and hot-path reads never re-read layout', () => {
    const { canvas, overlay, canvasSpy, overlaySpy } = makeElements()
    const rects = new ViewRects()
    rects.refresh(canvas, overlay)
    // the lock: MOVE-rate reads (hundreds/sec while dragging) must not hit
    // getBoundingClientRect — only the explicit refresh does
    for (let i = 0; i < 100; i++) {
      rects.normalized(200, 500, canvas)
      rects.local(200, 500)
    }
    expect(canvasSpy).toHaveBeenCalledTimes(1)
    expect(overlaySpy).toHaveBeenCalledTimes(1)
  })

  it('normalized maps client coords against the cached canvas rect', () => {
    const { canvas, overlay } = makeElements()
    const rects = new ViewRects()
    rects.refresh(canvas, overlay)
    expect(rects.normalized(200, 500, canvas)).toEqual({ x: 0.5, y: 0.5 })
  })

  it('local maps client coords into overlay-local pixels', () => {
    const { canvas, overlay } = makeElements()
    const rects = new ViewRects()
    rects.refresh(canvas, overlay)
    // overlay rect starts at y=0 while the canvas starts at y=100
    expect(rects.local(30, 150)).toEqual({ lx: 30, ly: 150 })
  })

  it('invalidate drops the cache until the next refresh', () => {
    const { canvas, overlay, canvasSpy } = makeElements()
    const rects = new ViewRects()
    rects.refresh(canvas, overlay)
    rects.invalidate()
    expect(rects.normalized(0, 0, canvas)).toBeNull()
    rects.refresh(canvas, overlay)
    expect(rects.normalized(200, 500, canvas)).toEqual({ x: 0.5, y: 0.5 })
    expect(canvasSpy).toHaveBeenCalledTimes(2)
  })
})
