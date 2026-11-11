import { readFileSync } from 'node:fs'
import { expect, it } from 'vitest'
import { WebSocketCrypto } from '../../src/lib/api/websocket-crypto'
import { parseWebSocketData } from '../../src/lib/api/sjcl-arraybuffer'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { fields, hash, text } from '../helpers/websocket-server'

const vectors = JSON.parse(readFileSync(new URL('../../../plain-core/plain-rs/testdata/crypto/websocket-event-vectors.json', import.meta.url), 'utf8')) as Array<{
  key: string; client: string; path: string; init: string; ack: string
  clientFrames: Array<{ frame: string; plain: string }>
  serverFrames: Array<{ frame: string; plain: string }>
}>
const bytes = (value: string) => new Uint8Array(Buffer.from(value, 'base64'))

it('matches Rust cross-language handshakes and both ordered directions for all socket paths', () => {
  for (const vector of vectors) {
    const key = bytes(vector.key)
    const crypto = new WebSocketCrypto(vector.client, key, vector.path)
    Object.defineProperty(crypto, 'registration', { value: bytes(vector.init) })
    crypto.acknowledge(bytes(vector.ack))
    const binding = hash(fields(bytes(vector.init), bytes(vector.ack)))
    for (const expected of vector.clientFrames) {
      const frame = crypto.encode(bytes(expected.plain))
      const aad = fields(text('plain-ws-frame'), text(vector.path), text(vector.client), binding, text('client'), frame.subarray(1, 9))
      expect(xchacha20poly1305(key, frame.subarray(9, 33), aad).decrypt(frame.subarray(33))).toEqual(bytes(expected.plain))
      expect(frame.subarray(0, 9)).toEqual(bytes(expected.frame).subarray(0, 9))
    }
    for (const expected of vector.serverFrames) {
      const plain = crypto.decode(bytes(expected.frame))
      expect(plain).toEqual(bytes(expected.plain))
      if (vector.path !== '/graphql') expect(parseWebSocketData(plain).type).toMatch(/^[A-Z_]+$/)
      expect(() => crypto.decode(bytes(expected.frame))).toThrow()
    }
  }
})
