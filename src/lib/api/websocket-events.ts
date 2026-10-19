import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { sha256 } from '@noble/hashes/sha2.js'
import { parseWebSocketData } from './sjcl-arraybuffer'

export function eventAad(name: string): Uint8Array {
  return new TextEncoder().encode(`plain-event-v1\0${name}`)
}

export class EventStream {
  private readonly binding: Uint8Array
  private next = 0n

  constructor(registration: Uint8Array) {
    this.binding = sha256(registration)
  }

  decode(frame: ArrayBuffer | Uint8Array, key: Uint8Array): { type: string; data: Uint8Array } {
    const { type, data } = parseWebSocketData(frame)
    if (data.length < 48) throw new Error('Invalid event frame')
    const sequence = new DataView(data.buffer, data.byteOffset, 8).getBigUint64(0)
    if (sequence !== this.next || sequence === 0xffffffffffffffffn) throw new Error('Invalid event sequence')
    const prefix = new TextEncoder().encode('plain-ws-event-v1\0')
    const name = new TextEncoder().encode(type)
    const aad = new Uint8Array(prefix.length + 32 + 8 + name.length)
    aad.set(prefix)
    aad.set(this.binding, prefix.length)
    aad.set(data.subarray(0, 8), prefix.length + 32)
    aad.set(name, prefix.length + 40)
    const plain = xchacha20poly1305(key, data.subarray(8, 32), aad).decrypt(data.subarray(32))
    this.next++
    return { type, data: plain }
  }
}
