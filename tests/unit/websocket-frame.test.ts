import { expect, it } from 'vitest'
import { encodeSocketPayload } from '../../src/lib/api/websocket-frame'

it('prefixes binary controls with exact big endian milliseconds without changing input bytes', () => {
  const payload = Uint8Array.of(0x54, 1, 0, 255)
  const frame = encodeSocketPayload(payload, 100000)
  expect([...frame]).toEqual([0, 0, 0, 0, 0, 1, 134, 160, 0x54, 1, 0, 255])
  expect([...payload]).toEqual([0x54, 1, 0, 255])
})

it('preserves UTF8 JSON control bytes', () => {
  const payload = new TextEncoder().encode('{"type":"screenMirrorControl","input":{"action":"BACK"}}')
  expect(encodeSocketPayload(payload, 100000).slice(8)).toEqual(payload)
})
