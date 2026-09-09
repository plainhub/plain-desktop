import { describe, expect, it } from 'vitest'
import {
  CaptureSelection,
  captureToolbarWorkArea,
  findSelectionHandle,
  handleCenters,
  placeCaptureToolbar,
  resizeSelection,
  type FrameBounds,
  type FramePoint,
  type SelectionHandle,
  type SelectionRect,
} from '@/views/screen-capture/selection-model'

const bounds: FrameBounds = { width: 100, height: 80 }

describe('CaptureSelection', () => {
  it('normalizes a new drag in any direction and clamps it to the frame', () => {
    const selection = new CaptureSelection(bounds)

    selection.begin(1, { x: 90, y: 70 })
    selection.update(1, { x: -20, y: -10 })
    selection.end(1)

    expect(selection.rect).toEqual({ x: 0, y: 0, width: 90, height: 70 })
  })

  it('moves an existing selection without changing its size or leaving the frame', () => {
    const selection = new CaptureSelection(bounds, { x: 20, y: 15, width: 40, height: 30 })

    selection.begin(1, { x: 30, y: 25 })
    selection.update(1, { x: 200, y: 200 })
    selection.end(1)

    expect(selection.rect).toEqual({ x: 60, y: 50, width: 40, height: 30 })
  })

  it('restores the prior rectangle when an interaction is cancelled', () => {
    const original: SelectionRect = { x: 20, y: 15, width: 40, height: 30 }
    const selection = new CaptureSelection(bounds, original)

    selection.begin(1, { x: 30, y: 25 })
    selection.update(1, { x: 70, y: 60 })
    selection.cancelInteraction(1)

    expect(selection.rect).toEqual(original)
  })

  it('drops click-sized selections instead of creating a zero-area export', () => {
    const selection = new CaptureSelection(bounds)
    selection.begin(1, { x: 10, y: 10 })
    selection.update(1, { x: 10.5, y: 10.5 })
    selection.end(1)

    expect(selection.rect).toBeNull()
  })

  it('commits exactly the 20 by 20 minimum and ignores another pointer', () => {
    const selection = new CaptureSelection(bounds)
    selection.begin(4, { x: 10, y: 10 })
    selection.update(5, { x: 90, y: 70 })
    selection.end(5)
    expect(selection.rect).toEqual({ x: 10, y: 10, width: 0, height: 0 })

    selection.update(4, { x: 30, y: 30 })
    selection.end(4)
    expect(selection.rect).toEqual({ x: 10, y: 10, width: 20, height: 20 })
  })

  it('rejects non-finite frame input', () => {
    const selection = new CaptureSelection(bounds)
    expect(() => selection.begin(1, { x: Number.NaN, y: 1 })).toThrow(/point/i)
  })

  it('rejects invalid frame bounds and committed initial rectangles', () => {
    expect(() => new CaptureSelection({ width: 10.5, height: 80 })).toThrow(/bounds/i)
    expect(() => new CaptureSelection(bounds, { x: -1, y: 0, width: 20, height: 20 })).toThrow(/selection/i)
    expect(() => new CaptureSelection(bounds, { x: 0, y: 0, width: 19, height: 20 })).toThrow(/selection/i)
    expect(() => new CaptureSelection(bounds, { x: 90, y: 0, width: 20, height: 20 })).toThrow(/selection/i)
  })

  it('keeps committed rectangles integer-normalized and inside the frame across a deterministic point matrix', () => {
    let pointerId = 1
    for (const startX of [-30, 0, 17.25, 99, 140]) {
      for (const startY of [-20, 0, 31.75, 79, 120]) {
        const selection = new CaptureSelection(bounds)
        selection.begin(pointerId, { x: startX, y: startY })
        selection.update(pointerId, { x: 100 - startX, y: 80 - startY })
        selection.end(pointerId)
        pointerId += 1
        const rect = selection.rect
        if (!rect) continue
        expect(Object.values(rect).every(Number.isInteger)).toBe(true)
        expect(rect.x).toBeGreaterThanOrEqual(0)
        expect(rect.y).toBeGreaterThanOrEqual(0)
        expect(rect.x + rect.width).toBeLessThanOrEqual(bounds.width)
        expect(rect.y + rect.height).toBeLessThanOrEqual(bounds.height)
        expect(rect.width).toBeGreaterThanOrEqual(20)
        expect(rect.height).toBeGreaterThanOrEqual(20)
      }
    }
  })
})

describe('selection handles', () => {
  const rect: SelectionRect = { x: 20, y: 10, width: 40, height: 30 }

  it('provides all eight WeChat-style resize handles', () => {
    expect(handleCenters(rect)).toEqual({
      nw: { x: 20, y: 10 },
      n: { x: 40, y: 10 },
      ne: { x: 60, y: 10 },
      e: { x: 60, y: 25 },
      se: { x: 60, y: 40 },
      s: { x: 40, y: 40 },
      sw: { x: 20, y: 40 },
      w: { x: 20, y: 25 },
    })
  })

  it.each<[SelectionHandle, FramePoint, SelectionRect]>([
    ['nw', { x: 10, y: 5 }, { x: 10, y: 5, width: 50, height: 35 }],
    ['n', { x: 50, y: 5 }, { x: 20, y: 5, width: 40, height: 35 }],
    ['ne', { x: 70, y: 5 }, { x: 20, y: 5, width: 50, height: 35 }],
    ['e', { x: 70, y: 20 }, { x: 20, y: 10, width: 50, height: 30 }],
    ['se', { x: 70, y: 50 }, { x: 20, y: 10, width: 50, height: 40 }],
    ['s', { x: 40, y: 50 }, { x: 20, y: 10, width: 40, height: 40 }],
    ['sw', { x: 10, y: 50 }, { x: 10, y: 10, width: 50, height: 40 }],
    ['w', { x: 10, y: 20 }, { x: 10, y: 10, width: 50, height: 30 }],
  ])('resizes handle %s in frame coordinates', (handle, point, expected) => {
    expect(resizeSelection(rect, handle, point, bounds)).toEqual(expected)
  })

  it('allows a corner to cross its opposite anchor while remaining normalized', () => {
    expect(resizeSelection(rect, 'nw', { x: 75, y: 55 }, bounds)).toEqual({ x: 60, y: 40, width: 20, height: 20 })
  })

  it('hit-tests constant-radius handle centers independently from frame size', () => {
    expect(findSelectionHandle(rect, { x: 62, y: 40 }, 4)).toBe('se')
    expect(findSelectionHandle(rect, { x: 50, y: 30 }, 4)).toBeNull()
  })

  it('preserves normalized, bounded minimum-size rectangles across every handle and edge', () => {
    const handles = Object.keys(handleCenters(rect)) as SelectionHandle[]
    const points = [-50, 0, 19.25, 40, 79.75, 100, 150]

    for (const handle of handles) {
      for (const x of points) {
        for (const y of points) {
          const resized = resizeSelection(rect, handle, { x, y }, bounds)
          expect(Object.values(resized).every(Number.isInteger)).toBe(true)
          expect(resized.x).toBeGreaterThanOrEqual(0)
          expect(resized.y).toBeGreaterThanOrEqual(0)
          expect(resized.x + resized.width).toBeLessThanOrEqual(bounds.width)
          expect(resized.y + resized.height).toBeLessThanOrEqual(bounds.height)
          expect(resized.width).toBeGreaterThanOrEqual(20)
          expect(resized.height).toBeGreaterThanOrEqual(20)
        }
      }
    }
  })
})

describe('capture toolbar placement', () => {
  it('prefers below, flips above, and clamps to the viewport', () => {
    expect(placeCaptureToolbar({ x: 20, y: 10, width: 40, height: 20 }, { width: 50, height: 12 }, bounds, 6)).toEqual({ x: 15, y: 36 })
    expect(placeCaptureToolbar({ x: 80, y: 65, width: 15, height: 12 }, { width: 50, height: 12 }, bounds, 6)).toEqual({ x: 50, y: 47 })
  })

  it('uses the visible screen work area and flips above a Dock-obscured bottom edge', () => {
    const viewport = { width: 1000, height: 800 }
    const workArea = captureToolbarWorkArea(viewport, { x: 0, y: 0 }, { x: 0, y: 24, width: 1000, height: 676 })

    expect(workArea).toEqual({ x: 0, y: 24, width: 1000, height: 676 })
    expect(placeCaptureToolbar({ x: 100, y: 500, width: 700, height: 180 }, { width: 620, height: 56 }, viewport, 8, workArea)).toEqual({ x: 140, y: 436 })
  })

  it('tries a horizontal side when neither vertical side is visible', () => {
    expect(placeCaptureToolbar({ x: 10, y: 0, width: 40, height: 100 }, { width: 50, height: 20 }, { width: 200, height: 100 }, 8, { x: 0, y: 0, width: 200, height: 100 })).toEqual({ x: 58, y: 40 })
  })

  it('intersects offset multi-monitor work areas and keeps the fallback fully visible', () => {
    const workArea = captureToolbarWorkArea({ width: 100, height: 80 }, { x: -1000, y: 40 }, { x: -990, y: 45, width: 80, height: 60 })

    expect(workArea).toEqual({ x: 10, y: 5, width: 80, height: 60 })
    expect(placeCaptureToolbar({ x: 0, y: 0, width: 100, height: 80 }, { width: 60, height: 12 }, bounds, 8, workArea)).toEqual({ x: 20, y: 53 })
  })
})
