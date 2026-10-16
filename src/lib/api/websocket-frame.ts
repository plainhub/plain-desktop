export function encodeSocketPayload(payload: Uint8Array, nowMillis = Date.now()): Uint8Array {
  const frame = new Uint8Array(8 + payload.length)
  new DataView(frame.buffer).setBigInt64(0, BigInt(nowMillis), false)
  frame.set(payload, 8)
  return frame
}
