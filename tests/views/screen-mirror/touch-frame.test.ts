import { describe, expect, it } from 'vitest'
import { TOUCH_ACTION_DOWN, TOUCH_ACTION_MOVE, TOUCH_ACTION_UP, encodeTouchFrame } from '@/views/screen-mirror/touch-frame'

describe('encodeTouchFrame', () => {
  it('writes the spec header: magic 0x54, count, streamId little-endian', () => {
    const frame = encodeTouchFrame([
      { action: TOUCH_ACTION_DOWN, pointerId: 1, x: 0.5, y: 0.5, dtMs: 0 },
    ])
    expect(frame[0]).toBe(0x54)
    expect(frame[1]).toBe(1)
    expect(frame[2]).toBe(0) // streamId low byte
    expect(frame[3]).toBe(0) // streamId high byte
    expect(frame).toHaveLength(4 + 8)
  })

  it('matches the plain-app decoder golden frame byte for byte (API_SPEC §12.4)', () => {
    // DOWN(0.5, 0.25, pointerId=1) + MOVE(0.75, 0.5, dt=16) — the exact byte
    // sequence TouchFrameDecodeTest on the phone asserts as well.
    const frame = encodeTouchFrame([
      { action: TOUCH_ACTION_DOWN, pointerId: 1, x: 0.5, y: 0.25, dtMs: 0 },
      { action: TOUCH_ACTION_MOVE, pointerId: 1, x: 0.75, y: 0.5, dtMs: 16 },
    ])
    expect(Array.from(frame)).toEqual([
      0x54, 0x02, 0x00, 0x00,
      0x00, 0x01, 0x00, 0x80, 0x00, 0x40, 0x00, 0x00,
      0x01, 0x01, 0xff, 0xbf, 0x00, 0x80, 0x10, 0x00,
    ])
  })

  it('encodes coordinates little-endian in the 0..65535 range', () => {
    const frame = encodeTouchFrame([
      { action: TOUCH_ACTION_MOVE, pointerId: 0, x: 0x1234 / 65535, y: 0x5678 / 65535, dtMs: 0 },
    ])
    expect(frame[6]).toBe(0x34)
    expect(frame[7]).toBe(0x12)
    expect(frame[8]).toBe(0x78)
    expect(frame[9]).toBe(0x56)
  })

  it('clamps coordinates into 0..1 and dtMs into u16', () => {
    const frame = encodeTouchFrame([
      { action: TOUCH_ACTION_MOVE, pointerId: 0, x: 1.5, y: -1, dtMs: 70000 },
    ])
    expect(frame[6]).toBe(0xff)
    expect(frame[7]).toBe(0xff)
    expect(frame[8]).toBe(0)
    expect(frame[9]).toBe(0)
    expect(frame[10]).toBe(0xff)
    expect(frame[11]).toBe(0xff)
  })

  it('masks pointerId into one byte', () => {
    const frame = encodeTouchFrame([
      { action: TOUCH_ACTION_DOWN, pointerId: 300, x: 0, y: 0, dtMs: 0 },
    ])
    expect(frame[5]).toBe(300 & 0xff)
  })

  it('encodes every action code', () => {
    const frame = encodeTouchFrame([
      { action: TOUCH_ACTION_DOWN, pointerId: 1, x: 0, y: 0, dtMs: 0 },
      { action: TOUCH_ACTION_MOVE, pointerId: 1, x: 0, y: 0, dtMs: 0 },
      { action: TOUCH_ACTION_UP, pointerId: 1, x: 0, y: 0, dtMs: 0 },
      { action: 3, pointerId: 1, x: 0, y: 0, dtMs: 0 }, // CANCEL
    ])
    expect(frame[1]).toBe(4)
    expect(frame[4]).toBe(0)
    expect(frame[12]).toBe(1)
    expect(frame[20]).toBe(2)
    expect(frame[28]).toBe(3)
  })
})
