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
      const aad = fields(text('WS_FRAME'), text(vector.path), text(vector.client), binding, text('client'), frame.subarray(0, 8))
      expect(xchacha20poly1305(key, frame.subarray(8, 32), aad).decrypt(frame.subarray(32))).toEqual(bytes(expected.plain))
      expect(frame.subarray(0, 8)).toEqual(bytes(expected.frame).subarray(0, 8))
    }
    for (const expected of vector.serverFrames) {
      const plain = crypto.decode(bytes(expected.frame))
      expect(plain).toEqual(bytes(expected.plain))
      if (vector.path !== '/graphql') expect(parseWebSocketData(plain).type).toMatch(/^[A-Z_]+$/)
      expect(() => crypto.decode(bytes(expected.frame))).toThrow()
    }
  }
})

it('matches Rust handshake nonce and object validation vectors', () => {
  const cases = JSON.parse(readFileSync(new URL('../../../plain-core/plain-rs/testdata/crypto/websocket-handshake-vectors.json', import.meta.url), 'utf8')) as Array<{ name: string; body: unknown; valid: boolean }>
  for (const path of ['/graphql', '/screen', '/image_editor']) {
    for (const entry of cases) {
      const key = new Uint8Array(32).fill(7)
      const crypto = new WebSocketCrypto('client', key, path)
      const aad = fields(text('WS_ACK'), text(path), text('client'), hash(crypto.registration))
      const nonce = new Uint8Array(24).fill(42)
      const cipher = xchacha20poly1305(key, nonce, aad).encrypt(text(JSON.stringify(entry.body)))
      const ack = new Uint8Array(nonce.length + cipher.length)
      ack.set(nonce)
      ack.set(cipher, nonce.length)
      if (entry.valid) expect(() => crypto.acknowledge(ack), entry.name).not.toThrow()
      else {
        expect(() => crypto.acknowledge(ack), entry.name).toThrow()
        expect(() => crypto.encode(new Uint8Array()), entry.name).toThrow('handshake incomplete')
      }
    }
  }
})
