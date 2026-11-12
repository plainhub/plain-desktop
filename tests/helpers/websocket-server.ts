import { createHash } from 'node:crypto'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'

export const text = (value: string) => new TextEncoder().encode(value)
export const hash = (value: Uint8Array) => createHash('sha256').update(value).digest()
export function fields(...values: Uint8Array[]): Buffer {
  return Buffer.concat(values.flatMap(value => {
    const length = Buffer.alloc(4)
    length.writeUInt32BE(value.length)
    return [length, value]
  }))
}
export function server(key: Uint8Array, registration: Uint8Array, path: string, client: string, seed = 9) {
  const initAad = fields(text('WS_INIT'), text(path), text(client))
  const init = JSON.parse(new TextDecoder().decode(xchacha20poly1305(key, registration.subarray(0, 24), initAad).decrypt(registration.subarray(24))))
  const nonce = new Uint8Array(24).fill(seed)
  const ackAad = fields(text('WS_ACK'), text(path), text(client), hash(registration))
  const encrypted = xchacha20poly1305(key, nonce, ackAad).encrypt(text(JSON.stringify({ serverNonce: seed.toString(16).padStart(2, '0').repeat(32) })))
  const ack = Uint8Array.from([...nonce, ...encrypted])
  const binding = hash(fields(registration, ack))
  const aad = (direction: string, sequence: Uint8Array) => fields(text('WS_FRAME'), text(path), text(client), binding, text(direction), sequence)
  return {
    init, ack,
    frame(sequence: bigint, plain: Uint8Array, direction = 'server') {
      const prefix = Buffer.alloc(8)
      prefix.writeBigUInt64BE(sequence)
      const nonce = new Uint8Array(24).fill(seed + 1)
      return Uint8Array.from(Buffer.concat([prefix, nonce, xchacha20poly1305(key, nonce, aad(direction, prefix)).encrypt(plain)]))
    },
    decode(frame: Uint8Array) {
      return xchacha20poly1305(key, frame.subarray(8, 32), aad('client', frame.subarray(0, 8))).decrypt(frame.subarray(32))
    },
  }
}
