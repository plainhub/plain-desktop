import { describe, expect, it } from 'vitest'
import { parseWebSocketData } from '@/lib/api/sjcl-arraybuffer'

const encoder = new TextEncoder()
function frame(name: string, payload: Uint8Array) {
  const header = encoder.encode(`${name}\0`)
  const bytes = new Uint8Array(header.length + payload.length)
  bytes.set(header)
  bytes.set(payload, header.length)
  return bytes.buffer
}

describe('named WebSocket frames', () => {
  it('dispatches the wire name and keeps a zero-copy binary payload', () => {
    for (const name of ['PAIRING_REQUEST_RECEIVED', 'SCREEN_MIRROR_VIDEO', 'SCREEN_MIRROR_AUDIO', 'IMAGE_EDITOR_UPDATE']) {
      const buffer = frame(name, new Uint8Array([0, 1, 255, 0]))
      const parsed = parseWebSocketData(buffer)
      expect(parsed.type).toBe(name)
      expect([...parsed.data]).toEqual([0, 1, 255, 0])
      expect(parsed.data.buffer).toBe(buffer)
    }
  })

  it('rejects numeric headers and malformed event names', () => {
    for (const bytes of [new Uint8Array([0, 0, 0, 22, 1]), encoder.encode('PAIRING_STARTED'), encoder.encode('22\0x'), encoder.encode('pairing_started\0x'), new Uint8Array([255, 0, 1])]) {
      expect(() => parseWebSocketData(bytes.buffer)).toThrow()
    }
  })
})
