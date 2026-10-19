import { hkdf } from '@noble/hashes/hkdf.js'
import { sha256 } from '@noble/hashes/sha2.js'
import { bitArrayToBase64, chachaDecrypt, chachaEncrypt, computeECDHSharedKey, generateECDHKeyPair, verifyEd25519Signature } from './crypto'
import { getSyncedTimestamp } from './time-sync'

const PROTOCOL = 'plain-web-login-v1'

interface ServerHello {
  protocol: string
  clientId: string
  deviceId: string
  clientPublicKey: string
  serverPublicKey: string
  clientNonce: string
  serverNonce: string
  expiresAt: number
  signature: string
}

function canonical(value: string, length: number): Uint8Array {
  const bytes = Uint8Array.from(atob(value), (character) => character.charCodeAt(0))
  if (bytes.length !== length || bitArrayToBase64(bytes) !== value) throw 'invalid_auth_response'
  return bytes
}

export function startLoginChannel(clientId: string) {
  const key = generateECDHKeyPair()
  const publicKey = bitArrayToBase64(key.publicKey)
  const nonce = bitArrayToBase64(crypto.getRandomValues(new Uint8Array(32)))
  return {
    hello: { protocol: PROTOCOL, publicKey, nonce },
    accept(response: ServerHello, signaturePublicKey: string, expectedDeviceId?: string) {
      const now = getSyncedTimestamp()
      if (response.protocol !== PROTOCOL || response.clientId !== clientId || response.clientPublicKey !== publicKey || response.clientNonce !== nonce) throw 'invalid_auth_response'
      if (typeof response.deviceId !== 'string' || !response.deviceId || response.deviceId.length > 256 || (expectedDeviceId && response.deviceId !== expectedDeviceId)) throw 'peer_identity_changed'
      if (!Number.isSafeInteger(response.expiresAt) || response.expiresAt <= now) throw 'login_expired'
      const serverPublic = canonical(response.serverPublicKey, 65)
      canonical(response.serverNonce, 32)
      canonical(signaturePublicKey, 32)
      const transcript = JSON.stringify([response.protocol, response.clientId, response.deviceId, response.clientPublicKey, response.serverPublicKey, response.clientNonce, response.serverNonce, response.expiresAt])
      if (!verifyEd25519Signature(signaturePublicKey, transcript, response.signature)) throw 'signature_verification_failed'
      const context = sha256(new TextEncoder().encode(transcript))
      const shared = canonical(computeECDHSharedKey(key.secretKey, serverPublic), 32)
      const requestKey = hkdf(sha256, shared, context, new TextEncoder().encode(`${PROTOCOL}/request`), 32)
      const responseKey = hkdf(sha256, shared, context, new TextEncoder().encode(`${PROTOCOL}/response`), 32)
      const expiresAt = Math.min(response.expiresAt, now + 300000)
      const fresh = () => { if (getSyncedTimestamp() >= expiresAt) throw 'login_expired' }
      const verificationCode = Array.from(context.slice(0, 16), (byte) => byte.toString(16).padStart(2, '0')).join('').match(/.{8}/g)!.join(' ')
      return {
        context: bitArrayToBase64(context),
        verificationCode,
        encryptRequest(request: string) { fresh(); return chachaEncrypt(requestKey, request) },
        decryptResponse(frame: Uint8Array) { fresh(); return chachaDecrypt(responseKey, frame) },
      }
    },
  }
}
