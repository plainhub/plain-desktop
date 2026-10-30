#!/usr/bin/env node
/**
 * Generates cross-platform crypto test vectors using the web's crypto
 * libraries (@noble/curves, @noble/ciphers, @noble/hashes) and writes
 * them to plain-app's commonTest resources directory.
 *
 * These vectors are consumed by WebCryptoVectorTest.kt in plain-app's
 * commonTest to verify that plain-app (Kotlin / Tink / BouncyCastle) can
 * decrypt/verify web-generated crypto material.
 *
 * Usage:
 *   node scripts/generate-web-crypto-vectors.mjs
 *
 * Output:
 *   plain-app/shared/src/commonTest/resources/web-crypto-vectors.json
 */
import { TextEncoder } from 'node:util'
import { p256 } from '@noble/curves/nist.js'
import { ed25519 } from '@noble/curves/ed25519.js'
import { sha256 } from '@noble/hashes/sha2.js'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { writeFileSync, mkdirSync } from 'node:fs'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const outputPath = resolve(
  __dirname,
  '../../plain-app/shared/src/commonTest/resources/web-crypto-vectors.json',
)

const b64 = (bytes) => Buffer.from(bytes).toString('base64')
const utf8 = (s) => new TextEncoder().encode(s)

// ── ECDH P-256 ───────────────────────────────────────────────────────────────
// Matches plain-app: secp256r1, X9.63 uncompressed public key (65 bytes),
// 32-byte raw private key, shared key = SHA-256(raw ECDH secret X-coordinate).

const privA = p256.utils.randomSecretKey()
const pubA = p256.getPublicKey(privA, false) // false = uncompressed (65 bytes)
const privB = p256.utils.randomSecretKey()
const pubB = p256.getPublicKey(privB, false)

const sharedAB = p256.getSharedSecret(privA, pubB)
const sharedX = sharedAB.subarray(1, 33) // X coordinate of the shared point
const derivedKey = sha256(sharedX)

// ── Ed25519 ──────────────────────────────────────────────────────────────────
// Matches plain-app: raw 32-byte private key, 32-byte public key, 64-byte sig.

const edPriv = ed25519.utils.randomSecretKey()
const edPub = ed25519.getPublicKey(edPriv)
const edMessage = 'web-generated ed25519 cross-platform test message'
const edSignature = ed25519.sign(utf8(edMessage), edPriv)

// ── XChaCha20-Poly1305 ───────────────────────────────────────────────────────
// Uses a FIXED nonce so the Kotlin test can verify the exact ciphertext.
// Format matches plain-app: ciphertext field includes the 16-byte Poly1305 tag.

const chachaKey = Uint8Array.from({ length: 32 }, (_, i) => (i * 11 + 5) & 0xFF)
const chachaNonce = Uint8Array.from({ length: 24 }, (_, i) => (i * 3 + 1) & 0xFF)
const chachaPlaintext = 'web-generated xchacha20-poly1305 cross-platform test'
const cipher = xchacha20poly1305(chachaKey, chachaNonce)
const chachaCiphertext = cipher.encrypt(utf8(chachaPlaintext))

// ── base64 / hex known-answer vectors ────────────────────────────────────────

const b64Input = [104, 101, 108, 108, 111] // "hello"
const b64Output = Buffer.from(b64Input).toString('base64')

const hexInput = [255, 0, 128]
const hexOutput = hexInput.map((b) => b.toString(16).padStart(2, '0')).join('')

// ── Write JSON ───────────────────────────────────────────────────────────────

const vectors = {
  ecdh: {
    privateKeyA: b64(privA),
    publicKeyA: b64(pubA),
    privateKeyB: b64(privB),
    publicKeyB: b64(pubB),
    sharedKey: b64(derivedKey),
  },
  ed25519: {
    privateKey: b64(edPriv),
    publicKey: b64(edPub),
    message: edMessage,
    signature: b64(edSignature),
  },
  xchacha20: {
    key: b64(chachaKey),
    nonce: b64(chachaNonce),
    plaintext: chachaPlaintext,
    ciphertext: b64(chachaCiphertext),
  },
  base64: {
    input: b64Input,
    output: b64Output,
  },
  hex: {
    input: hexInput,
    output: hexOutput,
  },
}

mkdirSync(dirname(outputPath), { recursive: true })
writeFileSync(outputPath, JSON.stringify(vectors, null, 2) + '\n')
console.log(`Web crypto vectors written to: ${outputPath}`)
