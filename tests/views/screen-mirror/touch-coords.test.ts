import { describe, expect, it } from 'vitest'
import { normalizeCoords } from '@/views/screen-mirror/touch-coords'

function makeCanvas(width: number, height: number) {
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  return canvas
}

describe('normalizeCoords', () => {
  it('maps the canvas CSS rect directly onto 0..1', () => {
    // rect origin (10, 20), 400×800 — the canvas rect IS the rendered video
    // rect (cover fit), so corners map exactly
    const rect = new DOMRect(10, 20, 400, 800)
    const canvas = makeCanvas(400, 800)
    expect(normalizeCoords(10, 20, rect, canvas)).toEqual({ x: 0, y: 0 })
    expect(normalizeCoords(410, 820, rect, canvas)).toEqual({ x: 1, y: 1 })
    expect(normalizeCoords(210, 420, rect, canvas)).toEqual({ x: 0.5, y: 0.5 })
  })

  it('clamps out-of-rect points instead of dropping them (screen-edge reachability)', () => {
    const rect = new DOMRect(0, 0, 400, 800)
    const canvas = makeCanvas(400, 800)
    expect(normalizeCoords(500, 400, rect, canvas)).toEqual({ x: 1, y: 0.5 })
    expect(normalizeCoords(-100, 400, rect, canvas)).toEqual({ x: 0, y: 0.5 })
    expect(normalizeCoords(200, 1000, rect, canvas)).toEqual({ x: 0.5, y: 1 })
    expect(normalizeCoords(200, -50, rect, canvas)).toEqual({ x: 0.5, y: 0 })
  })

  it('returns null when there is no video content or no rendered rect', () => {
    const rect = new DOMRect(0, 0, 400, 800)
    expect(normalizeCoords(0, 0, rect, makeCanvas(0, 800))).toBeNull()
    expect(normalizeCoords(0, 0, new DOMRect(0, 0, 0, 800), makeCanvas(400, 800))).toBeNull()
  })
})
