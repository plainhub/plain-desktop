import { sha512Hex } from './sha512'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { randomBytes } from '@noble/ciphers/webcrypto'
import { ed25519 } from '@noble/curves/ed25519.js'
import { p256 } from '@noble/curves/nist.js'
import { sha256 } from '@noble/hashes/sha2.js'

export function sha512(input: string): string {
  return sha512Hex(input)
}

/** Take first 32 ASCII chars of hex hash as a 32-byte key */
export function hashToKey(hash: string): Uint8Array {
  const sub = hash.substring(0, 32)
  return new Uint8Array(sub.split('').map((c) => c.charCodeAt(0)))
}

export function chachaEncrypt(key: Uint8Array, plaintext: string): Uint8Array {
  const nonce = randomBytes(24)
  const key32 = new Uint8Array(32)
  key32.set(key.slice(0, 32))
  const plaintextBytes = new TextEncoder().encode(plaintext)
  const cipher = xchacha20poly1305(key32, nonce)
  const ciphertext = cipher.encrypt(plaintextBytes)
  const result = new Uint8Array(nonce.length + ciphertext.length)
  result.set(nonce, 0)
  result.set(ciphertext, nonce.length)
  return result
}

export function chachaDecrypt(key: Uint8Array, data: Uint8Array): string {
  const nonce = data.slice(0, 24)
  const ciphertext = data.slice(24)
  const key32 = new Uint8Array(32)
  key32.set(key.slice(0, 32))
  const cipher = xchacha20poly1305(key32, nonce)
  const plaintext = cipher.decrypt(ciphertext)
  return new TextDecoder().decode(plaintext)
}

export function arrayBufferToBitArray(buffer: ArrayBuffer): Uint8Array {
  return new Uint8Array(buffer)
}

export function bitArrayToUint8Array(arr: Uint8Array): Uint8Array {
  return arr
}

export function bitArrayToBase64(arr: Uint8Array): string {
  return btoa(String.fromCharCode(...arr))
}

// ---- ECDH (P-256 / secp256r1) ----
// Uses @noble/curves pure-JS implementation — works without HTTPS (no crypto.subtle needed).
// Public keys are X9.63 uncompressed (65 bytes, 0x04 || X || Y) for Kotlin interop.

export interface ECDHKeyPair {
  secretKey: Uint8Array  // 32 bytes
  publicKey: Uint8Array  // 65 bytes, X9.63 uncompressed
}

export function generateECDHKeyPair(): ECDHKeyPair {
  const kp = p256.keygen()
  const publicKey = p256.getPublicKey(kp.secretKey, false) // uncompressed
  return { secretKey: kp.secretKey, publicKey }
}

export function computeECDHSharedKey(
  secretKey: Uint8Array,
  peerPublicKeyBytes: Uint8Array,
): string {
  const sharedSecret = p256.getSharedSecret(secretKey, peerPublicKeyBytes)
  // @noble returns the compressed point (0x02/0x03 prefix + X coordinate, 33 bytes).
  // Android `KeyAgreement.generateSecret` and iOS `SecKeyCopyKeyExchangeResult`
  // both return the bare 32-byte X coordinate. Drop the prefix to stay aligned.
  const xCoordinate = sharedSecret.slice(1)
  const hash = sha256(xCoordinate)
  return bitArrayToBase64(new Uint8Array(hash))
}

// ---- Ed25519 ----
// Uses @noble/curves pure-JS implementation — works without HTTPS.

export function verifyEd25519Signature(
  publicKeyBase64: string,
  data: string,
  signatureBase64: string,
): boolean {
  try {
    const pubBytes = Uint8Array.from(atob(publicKeyBase64), (c) => c.charCodeAt(0))
    const sigBytes = Uint8Array.from(atob(signatureBase64), (c) => c.charCodeAt(0))
    const dataBytes = new TextEncoder().encode(data)
    return ed25519.verify(sigBytes, dataBytes, pubBytes)
  } catch {
    return false
  }
}

export interface InitResponse {
  signaturePublicKey: string
  password?: string
  /** Set by plain-nas on first run: no password configured yet, the client
   *  must present a setup flow instead of the login form. */
  needsSetup?: boolean
}
