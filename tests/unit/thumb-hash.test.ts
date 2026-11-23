import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { thumbHashPreview } from '@/lib/media/thumb-hash'

beforeEach(() => {
  vi.stubGlobal('ImageData', class {
    readonly data: Uint8ClampedArray
    constructor(readonly width: number, readonly height: number) {
      this.data = new Uint8ClampedArray(width * height * 4)
    }
  })
})
afterEach(() => vi.unstubAllGlobals())

it('decodes an opaque ThumbHash and reuses its pixels across list renders', () => {
  const hash = '2BoDBwAAAAAAAAAAAAAAAAAAAAAAAAAA'
  const pixels = thumbHashPreview(hash)!
  expect([pixels.width, pixels.height]).toEqual([32, 32])
  expect(Array.from(pixels.data.slice(0, 4))).toEqual([199, 57, 35, 255])
  expect(thumbHashPreview(hash)).toBe(pixels)
})

it('preserves alpha and orientation for an encoded portrait', () => {
  const pixels = thumbHashPreview('U9iFCwgYgaCXd4h7ARn4aHN3cIiHeHg=')!
  expect([pixels.width, pixels.height]).toEqual([19, 32])
  expect(pixels.data.some((byte, offset) => offset % 4 === 3 && byte > 0 && byte < 255)).toBe(true)
})

it('rejects invalid and truncated hashes so the neutral placeholder can stay visible', () => {
  expect(thumbHashPreview('not*a*hash')).toBeUndefined()
  expect(thumbHashPreview('2BoDBwAA')).toBeUndefined()
})
