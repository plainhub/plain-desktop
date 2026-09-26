import { sha256 } from '@noble/hashes/sha2.js'
import { arrayBufferToBase64 } from '@/lib/strutil'

const domain = new TextEncoder().encode('plain-chat-pairing-v1')

export function deriveLoginChatKey(token: string): string {
  const tokenBytes = Uint8Array.from(atob(token), (c) => c.charCodeAt(0))
  const data = new Uint8Array(domain.length + tokenBytes.length)
  data.set(domain)
  data.set(tokenBytes, domain.length)
  return arrayBufferToBase64(sha256(data).buffer)
}
