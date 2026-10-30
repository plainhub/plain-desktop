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

export type LoginMode = 'password' | 'deviceConfirmation'

interface ServerHello {
  clientId: string
  deviceId: string
  clientCommitment: string
  serverPublicKey: string
  serverNonce: string
  expiresAt: number
  signature: string
}

function readBase64(value: string, length: number): Uint8Array {
  const bytes = base64ToArrayBuffer(value)
  if (bytes.length !== length || bitArrayToBase64(bytes) !== value) {
    throw 'invalid_auth_response'
  }
  return bytes
}

function formatVerificationCode(bytes: Uint8Array): string {
  let value = 0n
  for (const byte of bytes.slice(0, 8)) {
    value = (value << 8n) | BigInt(byte)
  }
  const digits = (value % 100000000n).toString().padStart(8, '0')
  return `${digits.slice(0, 4)} ${digits.slice(4)}`
}

interface ExpectedLoginHello {
  clientId: string
  commitment: string
  signaturePublicKey: string
  deviceId?: string
}

function verifyServerHello(
  response: ServerHello,
  expected: ExpectedLoginHello,
  domain: string,
  now: number
) {
  if (
    response.clientId !== expected.clientId ||
    response.clientCommitment !== expected.commitment
  ) {
    throw 'invalid_auth_response'
  }
  if (
    typeof response.deviceId !== 'string' ||
    !response.deviceId ||
    response.deviceId.length > 256 ||
    (expected.deviceId && response.deviceId !== expected.deviceId)
  ) {
    throw 'peer_identity_changed'
  }
  if (!Number.isSafeInteger(response.expiresAt) || response.expiresAt <= now) {
    throw 'login_expired'
  }

  const serverPublicKey = readBase64(response.serverPublicKey, 65)
  readBase64(response.serverNonce, 32)
  readBase64(expected.signaturePublicKey, 32)
  const signedFields = [
    domain,
    response.clientId,
    response.deviceId,
    response.clientCommitment,
    response.serverPublicKey,
    response.serverNonce,
    response.expiresAt,
  ]
  const signed = JSON.stringify(signedFields)
  if (!verifyEd25519Signature(expected.signaturePublicKey, signed, response.signature)) {
    throw 'signature_verification_failed'
  }
  return { signedFields, serverPublicKey }
}

function createLoginEncryption(
  sharedKey: Uint8Array,
  context: Uint8Array,
  domain: string,
  expiresAt: number
) {
  const encoder = new TextEncoder()
  const requestKey = hkdf(sha256, sharedKey, context, encoder.encode(`${domain}/request`), 32)
  const responseKey = hkdf(sha256, sharedKey, context, encoder.encode(`${domain}/response`), 32)
  const verificationKey = hkdf(sha256, sharedKey, context, encoder.encode(`${domain}/sas`), 32)
  const requireUnexpired = () => {
    if (getSyncedTimestamp() >= expiresAt) throw 'login_expired'
  }

  return {
    context: bitArrayToBase64(context),
    verificationCode: formatVerificationCode(verificationKey),
    encryptRequest(bytes: Uint8Array) {
      requireUnexpired()
      return chachaEncryptBytes(requestKey, bytes)
    },
    decryptResponse(bytes: Uint8Array) {
      requireUnexpired()
      return chachaDecryptBytes(responseKey, bytes)
    },
  }
}

export function startLoginKeyExchange(clientId: string, mode: LoginMode) {
  const domain = mode === 'password' ? 'plain-web-password-login-v1' : 'plain-web-login-v2'
  const key = generateECDHKeyPair()
  const publicKey = bitArrayToBase64(key.publicKey)
  const nonce = bitArrayToBase64(crypto.getRandomValues(new Uint8Array(32)))
  const encoder = new TextEncoder()
  const committedFields = JSON.stringify([domain, clientId, publicKey, nonce])
  const commitment = bitArrayToBase64(sha256(encoder.encode(committedFields)))
  let pending = true

  return {
    hello: { commitment },
    accept(response: ServerHello, signaturePublicKey: string, expectedDeviceId?: string) {
      if (!pending) throw 'invalid_auth_response'
      pending = false
      const now = getSyncedTimestamp()
      const expected = { clientId, commitment, signaturePublicKey, deviceId: expectedDeviceId }
      const { signedFields, serverPublicKey } = verifyServerHello(response, expected, domain, now)
      const exchange = JSON.stringify([...signedFields, publicKey, nonce])
      const context = sha256(encoder.encode(exchange))
      const sharedKey = readBase64(computeECDHSharedKey(key.secretKey, serverPublicKey), 32)
      const expiresAt = Math.min(response.expiresAt, now + 300000)
      return {
        reveal: { publicKey, nonce },
        ...createLoginEncryption(sharedKey, context, domain, expiresAt),
      }
    },
  }
}

export type LoginKeys = ReturnType<ReturnType<typeof startLoginKeyExchange>['accept']>
