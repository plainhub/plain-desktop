import { hkdf } from '@noble/hashes/hkdf.js'
import { sha256 } from '@noble/hashes/sha2.js'
import { bitArrayToBase64, chachaDecrypt, chachaEncrypt, computeECDHSharedKey, generateECDHKeyPair, verifyEd25519Signature } from './crypto'
import { getSyncedTimestamp } from './time-sync'

const DOMAIN = 'plain-web-login-v2'

interface ServerHello {
  clientId: string
  deviceId: string
  clientCommitment: string
  serverPublicKey: string
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
  const encoder = new TextEncoder()
  const commitment = bitArrayToBase64(sha256(encoder.encode(JSON.stringify([DOMAIN, clientId, publicKey, nonce]))))
  let pending = true
  return {
    hello: { commitment },
    accept(response: ServerHello, signaturePublicKey: string, expectedDeviceId?: string) {
      if (!pending) throw 'invalid_auth_response'
      pending = false
      const now = getSyncedTimestamp()
      if (response.clientId !== clientId || response.clientCommitment !== commitment) throw 'invalid_auth_response'
      if (typeof response.deviceId !== 'string' || !response.deviceId || response.deviceId.length > 256 || (expectedDeviceId && response.deviceId !== expectedDeviceId)) throw 'peer_identity_changed'
      if (!Number.isSafeInteger(response.expiresAt) || response.expiresAt <= now) throw 'login_expired'
      const serverPublic = canonical(response.serverPublicKey, 65)
      canonical(response.serverNonce, 32)
      canonical(signaturePublicKey, 32)
      const fields = [DOMAIN, response.clientId, response.deviceId, response.clientCommitment, response.serverPublicKey, response.serverNonce, response.expiresAt]
      if (!verifyEd25519Signature(signaturePublicKey, JSON.stringify(fields), response.signature)) throw 'signature_verification_failed'
      const context = sha256(encoder.encode(JSON.stringify([...fields, publicKey, nonce])))
      const shared = canonical(computeECDHSharedKey(key.secretKey, serverPublic), 32)
      const requestKey = hkdf(sha256, shared, context, encoder.encode(`${DOMAIN}/request`), 32)
      const responseKey = hkdf(sha256, shared, context, encoder.encode(`${DOMAIN}/response`), 32)
      const sas = hkdf(sha256, shared, context, encoder.encode(`${DOMAIN}/sas`), 32)
      const value = sas.slice(0, 8).reduce((value, byte) => (value << 8n) | BigInt(byte), 0n) % 100000000n
      const digits = value.toString().padStart(8, '0')
      const expiresAt = Math.min(response.expiresAt, now + 300000)
      const fresh = () => { if (getSyncedTimestamp() >= expiresAt) throw 'login_expired' }
      return {
        reveal: { publicKey, nonce },
        context: bitArrayToBase64(context),
        verificationCode: `${digits.slice(0, 4)} ${digits.slice(4)}`,
        encryptRequest(request: string) { fresh(); return chachaEncrypt(requestKey, request) },
        decryptResponse(frame: Uint8Array) { fresh(); return chachaDecrypt(responseKey, frame) },
      }
    },
  }
}
