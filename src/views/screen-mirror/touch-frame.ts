// Binary touch-frame wire format, ported from plain-cast's low-latency
// gesture control (docs/touch-low-latency-design.md §3.2). Hot-path touch
// samples travel as one compact binary frame instead of per-event requests.
export const TOUCH_ACTION_DOWN = 0
export const TOUCH_ACTION_MOVE = 1
export const TOUCH_ACTION_UP = 2
export const TOUCH_ACTION_CANCEL = 3

const TOUCH_FRAME_MAGIC = 0x54

export interface TouchSample {
  action: number
  pointerId: number
  /** Normalized 0..1 within the mirrored video content. */
  x: number
  y: number
  /** Milliseconds since the previous sample of the same pointer stream. */
  dtMs: number
}

function clampU16(v: number): number {
  return Math.max(0, Math.min(65535, v))
}

/**
 * Serialize touch samples into one little-endian binary WS frame.
 * Header: u8 magic | u8 count | u16 streamId (always 0 for the mirror screen).
 */
export function encodeTouchFrame(samples: TouchSample[], streamId = 0): Uint8Array {
  const bytes = new Uint8Array(4 + samples.length * 8)
  const view = new DataView(bytes.buffer)
  view.setUint8(0, TOUCH_FRAME_MAGIC)
  view.setUint8(1, samples.length)
  view.setUint16(2, streamId, true)
  let offset = 4
  for (const s of samples) {
    view.setUint8(offset, s.action)
    view.setUint8(offset + 1, s.pointerId & 0xff)
    view.setUint16(offset + 2, clampU16(Math.round(s.x * 65535)), true)
    view.setUint16(offset + 4, clampU16(Math.round(s.y * 65535)), true)
    view.setUint16(offset + 6, clampU16(Math.round(s.dtMs)), true)
    offset += 8
  }
  return bytes
}
