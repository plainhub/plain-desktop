import { readFileSync } from 'node:fs'
import { expect, it } from 'vitest'
import { EventStream, eventAad } from '../../src/lib/api/websocket-events'
import { parseWebSocketData } from '../../src/lib/api/sjcl-arraybuffer'
import { chachaDecrypt } from '../../src/lib/api/crypto'

const vectors = JSON.parse(readFileSync(new URL('../../../plain-core/plain-rs/testdata/crypto/websocket-event-vectors.json', import.meta.url), 'utf8'))
const bytes = (value: string) => new Uint8Array(Buffer.from(value, 'base64'))

it('decodes every ordered Rust/Noble vector including exact binary bytes', () => {
  const stream = new EventStream(bytes(vectors[0].registration))
  for (const vector of vectors) {
    const key = bytes(vector.key)
    const frame = bytes(vector.frame)
    expect(stream.decode(frame, key)).toEqual({ type: vector.name, data: bytes(vector.payload) })
    expect(() => stream.decode(frame, key)).toThrow()
    const native = parseWebSocketData(bytes(vector.nativeFrame))
    expect(chachaDecrypt(key, native.data, eventAad(native.type))).toBe(new TextDecoder().decode(bytes(vector.payload)))
  }
})

it('rejects substitution, cross-connection replay, gaps, wrong keys and truncation without consuming a valid sequence', () => {
  const [first, second] = vectors
  const stream = new EventStream(bytes(first.registration))
  const key = bytes(first.key)
  const frame = bytes(first.frame)
  const body = parseWebSocketData(frame).data
  const renamed = Buffer.concat([Buffer.from('DEVICE_NAME_UPDATED\0'), body])
  expect(() => stream.decode(renamed, key)).toThrow()
  expect(() => new EventStream(new Uint8Array([1])).decode(frame, key)).toThrow()
  expect(() => stream.decode(frame, new Uint8Array(32))).toThrow()
  expect(() => stream.decode(bytes(second.frame), key)).toThrow()
  for (let length = 0; length < frame.length; length++) {
    expect(() => stream.decode(frame.subarray(0, length), key)).toThrow()
  }
  const changed = frame.slice()
  changed[changed.length - 1]! ^= 1
  expect(() => stream.decode(changed, key)).toThrow()
  expect(() => stream.decode(bytes(first.nativeFrame), key)).toThrow()
  expect(stream.decode(frame, key).type).toBe(first.name)
  expect(stream.decode(bytes(second.frame), key).data).toEqual(bytes(second.payload))
})
