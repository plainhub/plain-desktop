import { hkdf } from '@noble/hashes/hkdf.js'
import { sha256 } from '@noble/hashes/sha2.js'
import {
  bitArrayToBase64,
  chachaDecryptBytes,
  chachaEncryptBytes,
  computeECDHSharedKey,
  generateECDHKeyPair,
  verifyEd25519Signature,
} from './crypto'
import { getSyncedTimestamp } from './time-sync'
import { base64ToArrayBuffer } from '@/lib/strutil'

interface ServerHello {
  deviceId: string
  serverPublicKey: string
  expiresAt: number
  signature: string
}

function readBase64(value: string, length: number): Uint8Array {
  const bytes = base64ToArrayBuffer(value)
  if (bytes.length !== length || bitArrayToBase64(bytes) !== value) throw 'invalid_auth_response'
  return bytes
}

export function startLoginKeyExchange(clientId: string) {
  const session = generateECDHKeyPair()
  const hello = { publicKey: bitArrayToBase64(session.publicKey) }
  let pending = true
  const dispose = () => { pending = false; session.secretKey.fill(0) }
  return {
    hello,
    dispose,
    accept(response: ServerHello, signaturePublicKey: string, expectedDeviceId?: string) {
      if (!pending) throw 'invalid_auth_response'
      try {
        if (typeof response.deviceId !== 'string' || !response.deviceId || response.deviceId.length > 256 ||
          expectedDeviceId && response.deviceId !== expectedDeviceId) throw 'peer_identity_changed'
        const now = getSyncedTimestamp()
        if (!Number.isSafeInteger(response.expiresAt) || response.expiresAt <= now) throw 'login_expired'
        const peerKey = readBase64(response.serverPublicKey, 65)
        readBase64(signaturePublicKey, 32)
        const body = { deviceId: response.deviceId, serverPublicKey: response.serverPublicKey, expiresAt: response.expiresAt }
        const signed = JSON.stringify(['web-approval', 'responder', response.deviceId, signaturePublicKey, 'initiator', clientId, hello, body])
        if (!verifyEd25519Signature(signaturePublicKey, signed, response.signature)) throw 'signature_verification_failed'
        const encoder = new TextEncoder()
        const digest = sha256(encoder.encode(JSON.stringify(['web-approval', clientId, response.deviceId, hello, { ...body, signature: response.signature }])))
        const shared = readBase64(computeECDHSharedKey(session.secretKey, peerKey), 32)
        const key = hkdf(sha256, shared, digest, encoder.encode('api'), 32)
        shared.fill(0)
        const requestAad = new Uint8Array([...encoder.encode('auth-request'), ...digest])
        const responseAad = new Uint8Array([...encoder.encode('auth-response'), ...digest])
        const expiresAt = Math.min(response.expiresAt, now + 300000)
        const requireUnexpired = () => { if (getSyncedTimestamp() >= expiresAt) throw 'login_expired' }
        return {
          deviceId: response.deviceId,
          token: bitArrayToBase64(key),
          verificationCode: Array.from(digest, byte => byte.toString(16).padStart(2, '0')).join(''),
          encryptRequest(bytes: Uint8Array) { requireUnexpired(); return chachaEncryptBytes(key, bytes, requestAad) },
          decryptResponse(bytes: Uint8Array) { requireUnexpired(); return chachaDecryptBytes(key, bytes, responseAad) },
          dispose() { key.fill(0) },
        }
      } finally { dispose() }
    },
  }
}

export type LoginKeys = ReturnType<ReturnType<typeof startLoginKeyExchange>['accept']>
