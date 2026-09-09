import { describe, expect, it } from 'vitest'
import { cssPointToFrame, frameRectToCss, selectionToExportRect } from '@/views/screen-capture/capture-geometry'

describe('capture coordinate conversion', () => {
  it('maps CSS viewport points into physical frame pixels and clamps edges', () => {
    const viewport = { left: 10, top: 20, width: 1280, height: 720 }
    const frame = { width: 2560, height: 1440 }

    expect(cssPointToFrame({ x: 650, y: 380 }, viewport, frame)).toEqual({ x: 1280, y: 720 })
    expect(cssPointToFrame({ x: -100, y: 900 }, viewport, frame)).toEqual({ x: 0, y: 1440 })
  })

  it('maps a frame selection back to CSS without involving global monitor origins', () => {
    expect(frameRectToCss({ x: 512, y: 288, width: 1024, height: 576 }, { left: 0, top: 0, width: 1280, height: 720 }, { width: 2560, height: 1440 })).toEqual({
      x: 256,
      y: 144,
      width: 512,
      height: 288,
    })
  })

  it('rounds outward exactly once when producing integer export bounds', () => {
    expect(selectionToExportRect({ x: 10.75, y: 20.25, width: 40.5, height: 30.5 }, { width: 100, height: 80 })).toEqual({
      x: 10,
      y: 20,
      width: 42,
      height: 31,
    })
  })

  it('rejects degenerate viewports and selections', () => {
    expect(() => cssPointToFrame({ x: 0, y: 0 }, { left: 0, top: 0, width: 0, height: 100 }, { width: 100, height: 100 })).toThrow(/viewport/i)
    expect(() => selectionToExportRect({ x: 1, y: 1, width: 0, height: 1 }, { width: 100, height: 100 })).toThrow(/selection/i)
  })
})
