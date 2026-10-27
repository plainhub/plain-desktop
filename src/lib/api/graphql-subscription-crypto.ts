import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { randomBytes } from '@noble/ciphers/webcrypto'
import { sha256 } from '@noble/hashes/sha2.js'
import { getSyncedTimestamp } from './time-sync'

const utf8 = new TextEncoder()
function fields(values: Uint8Array[]): Uint8Array {
  const result = new Uint8Array(values.reduce((length, value) => length + 4 + value.length, 0))
  let offset = 0
  for (const value of values) {
    new DataView(result.buffer).setUint32(offset, value.length, false)
    result.set(value, offset + 4)
    offset += 4 + value.length
  }
  return result
}
function seal(key: Uint8Array, plain: Uint8Array, aad: Uint8Array): Uint8Array {
  const nonce = randomBytes(24)
  const cipher = xchacha20poly1305(key, nonce, aad).encrypt(plain)
  const result = new Uint8Array(24 + cipher.length)
  result.set(nonce)
  result.set(cipher, 24)
  return result
}
function open(key: Uint8Array, frame: Uint8Array, aad: Uint8Array): Uint8Array {
  return xchacha20poly1305(key, frame.subarray(0, 24), aad).decrypt(frame.subarray(24))
}
export class GraphqlSubscriptionCrypto {
  readonly registration: Uint8Array
  private binding: Uint8Array | undefined
  private sent = 0n
  private received = 0n
  constructor(private client: string, private key: Uint8Array) {
    const clientNonce = [...randomBytes(32)].map(value => value.toString(16).padStart(2, '0')).join('')
    const plain = utf8.encode(JSON.stringify({ timestampMillis: getSyncedTimestamp(), clientNonce }))
    const cipher = seal(key, plain, fields([utf8.encode('plain-graphql-init-v1'), utf8.encode('/graphql'), utf8.encode(client)]))
    this.registration = new Uint8Array(1 + cipher.length)
    this.registration[0] = 1
    this.registration.set(cipher, 1)
  }
  acknowledge(frame: Uint8Array) {
    if (frame[0] !== 1 || this.binding) throw new Error('Invalid subscription acknowledgement')
    const aad = fields([utf8.encode('plain-graphql-ack-v1'), utf8.encode('/graphql'), utf8.encode(this.client), sha256(this.registration)])
    const ack: { serverNonce: string } = JSON.parse(new TextDecoder().decode(open(this.key, frame.subarray(1), aad)))
    if (!/^[0-9a-f]{64}$/.test(ack.serverNonce)) throw new Error('Invalid subscription server nonce')
    this.binding = sha256(fields([this.registration, frame]))
  }
  private aad(direction: 'client' | 'server', sequence: bigint): Uint8Array {
    if (!this.binding) throw new Error('Subscription handshake incomplete')
    const counter = new Uint8Array(8)
    new DataView(counter.buffer).setBigUint64(0, sequence, false)
    return fields([utf8.encode('plain-graphql-frame-v1'), utf8.encode('/graphql'), utf8.encode(this.client), this.binding, utf8.encode(direction), counter])
  }
  encode(message: unknown): Uint8Array {
    if (this.sent === 0xffffffffffffffffn) throw new Error('Subscription sequence exhausted')
    const cipher = seal(this.key, utf8.encode(JSON.stringify(message)), this.aad('client', this.sent))
    const frame = new Uint8Array(9 + cipher.length)
    frame[0] = 1
    new DataView(frame.buffer).setBigUint64(1, this.sent, false)
    frame.set(cipher, 9)
    this.sent++
    return frame
  }
  decode(frame: Uint8Array): unknown {
    if (frame[0] !== 1 || frame.length < 49) throw new Error('Invalid subscription frame')
    const sequence = new DataView(frame.buffer, frame.byteOffset, frame.byteLength).getBigUint64(1, false)
    if (sequence !== this.received || sequence === 0xffffffffffffffffn) throw new Error('Invalid subscription sequence')
    const plain = open(this.key, frame.subarray(9), this.aad('server', sequence))
    this.received++
    return JSON.parse(new TextDecoder().decode(plain))
  }
}
