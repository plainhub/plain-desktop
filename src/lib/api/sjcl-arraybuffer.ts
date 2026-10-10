/** Convert an ArrayBuffer to a Uint8Array key/data representation. */
export function arrayBuffertoBits(buffer: ArrayBuffer): Uint8Array {
  return new Uint8Array(buffer)
}

/** Convert a Uint8Array back to an ArrayBuffer. */
export function arrayBufferFromBits(arr: Uint8Array): ArrayBuffer {
  // Ensure we return a plain ArrayBuffer (not SharedArrayBuffer)
  return arr.buffer.slice(arr.byteOffset, arr.byteOffset + arr.byteLength) as ArrayBuffer
}

export function parseWebSocketData(buffer: ArrayBuffer): { type: string; data: Uint8Array } {
  const bytes = new Uint8Array(buffer)
  const end = bytes.indexOf(0)
  if (end <= 0) throw new Error('Missing WebSocket event name')
  const type = new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(0, end))
  if (!/^[A-Z_]+$/.test(type)) throw new Error('Invalid WebSocket event name')
  return { type, data: bytes.subarray(end + 1) }
}
