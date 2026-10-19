import { readFileSync } from 'node:fs'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'

vi.mock('@/lib/api/crypto', async (original) => ({ ...await original<object>(), generateECDHKeyPair: vi.fn() }))
vi.mock('@/lib/api/time-sync', () => ({ getSyncedTimestamp: vi.fn(() => 1000) }))

import { generateECDHKeyPair } from '../../src/lib/api/crypto'
import { getSyncedTimestamp } from '../../src/lib/api/time-sync'
import { startLoginChannel } from '../../src/lib/api/login-channel'

const vector = JSON.parse(readFileSync(new URL('../../../plain-core/plain-rs/testdata/crypto/web-login-channel-vector.json', import.meta.url), 'utf8'))
const bytes = (value: string) => new Uint8Array(Buffer.from(value, 'base64'))
const start = () => startLoginChannel(vector.hello.clientId)

beforeEach(() => {
  vi.mocked(generateECDHKeyPair).mockReturnValue({ secretKey: bytes(vector.clientPrivate), publicKey: bytes(vector.hello.clientPublicKey) })
  vi.mocked(getSyncedTimestamp).mockReturnValue(1000)
  vi.spyOn(crypto, 'getRandomValues').mockImplementationOnce((target) => { new Uint8Array(target!.buffer).fill(3); return target })
})
afterEach(() => vi.restoreAllMocks())

it('matches the shared Rust transcript and both direction keys through actual encrypted frames', () => {
  const channel = start().accept(vector.hello, vector.signingPublicKey, 'fixture-device')
  expect(channel.context).toBe(vector.context)
  expect(channel.verificationCode).toMatch(/^[0-9a-f]{8}( [0-9a-f]{8}){3}$/)
  const request = channel.encryptRequest(vector.request)
  expect(new TextDecoder().decode(xchacha20poly1305(bytes(vector.requestKey), request.slice(0, 24)).decrypt(request.slice(24)))).toBe(vector.request)
  const nonce = new Uint8Array(24).fill(9)
  const response = xchacha20poly1305(bytes(vector.responseKey), nonce).encrypt(new TextEncoder().encode('approved'))
  expect(channel.decryptResponse(new Uint8Array([...nonce, ...response]))).toBe('approved')
  expect(() => channel.decryptResponse(bytes(vector.requestFrame))).toThrow()
})

it.each(['protocol', 'clientId', 'deviceId', 'clientPublicKey', 'serverPublicKey', 'clientNonce', 'serverNonce', 'signature'])('rejects a substituted %s', (field) => {
  const hello = { ...vector.hello, [field]: 'substituted' }
  expect(() => start().accept(hello, vector.signingPublicKey)).toThrow()
})

it('rejects replacement trusted identities and the advertised expiry boundary', () => {
  const client = start()
  expect(() => client.accept(vector.hello, Buffer.alloc(32, 8).toString('base64'))).toThrow()
  expect(() => client.accept(vector.hello, vector.signingPublicKey, 'other-device')).toThrow()
  const channel = client.accept(vector.hello, vector.signingPublicKey)
  vi.mocked(getSyncedTimestamp).mockReturnValue(301000)
  expect(() => client.accept(vector.hello, vector.signingPublicKey)).toThrow()
  expect(() => channel.encryptRequest('late request')).toThrow()
  expect(() => channel.decryptResponse(bytes(vector.requestFrame))).toThrow()
})

it('accepts a signed hello when page time trails the server and caps its local lifetime', () => {
  vi.mocked(getSyncedTimestamp).mockReturnValue(900)
  const channel = start().accept(vector.hello, vector.signingPublicKey, 'fixture-device')
  vi.mocked(getSyncedTimestamp).mockReturnValue(300899)
  expect(channel.encryptRequest('request')).toBeDefined()
  vi.mocked(getSyncedTimestamp).mockReturnValue(300900)
  expect(() => channel.encryptRequest('request')).toThrow()
})

it('rejects a changed signed expiry even when the new deadline is still fresh', () => {
  expect(() => start().accept({ ...vector.hello, expiresAt: vector.hello.expiresAt + 1 }, vector.signingPublicKey)).toThrow('signature_verification_failed')
})
