import { readFileSync } from 'node:fs'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'

vi.mock('@/lib/api/crypto', async original => ({ ...await original<object>(), generateECDHKeyPair: vi.fn() }))
vi.mock('@/lib/api/time-sync', () => ({ getSyncedTimestamp: vi.fn() }))
import { generateECDHKeyPair } from '../../src/lib/api/crypto'
import { getSyncedTimestamp } from '../../src/lib/api/time-sync'
import { startLoginKeyExchange } from '../../src/lib/api/login-channel'

const vector = JSON.parse(readFileSync(new URL('../../../plain-core/plain-rs/testdata/crypto/web-approval-vector.json', import.meta.url), 'utf8'))
const bytes = (value: string) => new Uint8Array(Buffer.from(value, 'base64'))
const now = vector.response.expiresAt - 300000
const aad = (response: boolean) => new Uint8Array([...new TextEncoder().encode(response ? 'auth-response' : 'auth-request'), ...Buffer.from(vector.fingerprint, 'hex')])
const start = () => startLoginKeyExchange(vector.clientId)
beforeEach(() => {
  vi.mocked(generateECDHKeyPair).mockImplementation(() => ({ secretKey: bytes(vector.clientPrivate), publicKey: bytes(vector.request.publicKey) }))
  vi.mocked(getSyncedTimestamp).mockReturnValue(now)
})
afterEach(() => vi.restoreAllMocks())

it('matches the independent Rust key and full fingerprint with direction-bound binary encryption', () => {
  const channel = start().accept(vector.response, vector.serverIdentity, 'fixture-device')
  expect(channel.token).toBe(vector.key)
  expect(channel.verificationCode).toBe(vector.fingerprint)
  expect(channel.verificationCode).toMatch(/^[0-9a-f]{64}$/)
  const request = channel.encryptRequest(new TextEncoder().encode('request'))
  expect(new TextDecoder().decode(xchacha20poly1305(bytes(vector.key), request.slice(0, 24), aad(false)).decrypt(request.slice(24)))).toBe('request')
  const nonce = new Uint8Array(24).fill(9)
  const response = xchacha20poly1305(bytes(vector.key), nonce, aad(true)).encrypt(new TextEncoder().encode('approved'))
  expect(new TextDecoder().decode(channel.decryptResponse(new Uint8Array([...nonce, ...response])))).toBe('approved')
  expect(() => channel.decryptResponse(request)).toThrow()
  channel.dispose()
})

it.each(['deviceId', 'serverPublicKey', 'signature'])('rejects a substituted %s', field => {
  expect(() => start().accept({ ...vector.response, [field]: 'substituted' }, vector.serverIdentity)).toThrow()
})

it('rejects replacement identities, client ids and repeated acceptance', () => {
  expect(() => start().accept(vector.response, Buffer.alloc(32, 8).toString('base64'))).toThrow()
  expect(() => start().accept(vector.response, vector.serverIdentity, 'other-device')).toThrow()
  expect(() => startLoginKeyExchange('other-client').accept(vector.response, vector.serverIdentity)).toThrow()
  const client = start()
  client.accept(vector.response, vector.serverIdentity).dispose()
  expect(() => client.accept(vector.response, vector.serverIdentity)).toThrow('invalid_auth_response')
})

it('expires at the boundary without extending an already accepted channel', () => {
  const channel = start().accept(vector.response, vector.serverIdentity)
  vi.mocked(getSyncedTimestamp).mockReturnValue(vector.response.expiresAt)
  expect(() => start().accept(vector.response, vector.serverIdentity)).toThrow('login_expired')
  expect(() => channel.encryptRequest(new Uint8Array())).toThrow('login_expired')
  channel.dispose()
})

it('caps the local lifetime when page time trails the server', () => {
  vi.mocked(getSyncedTimestamp).mockReturnValue(now - 100)
  const channel = start().accept(vector.response, vector.serverIdentity)
  vi.mocked(getSyncedTimestamp).mockReturnValue(vector.response.expiresAt - 101)
  expect(channel.encryptRequest(new TextEncoder().encode('request'))).toBeDefined()
  vi.mocked(getSyncedTimestamp).mockReturnValue(vector.response.expiresAt - 100)
  expect(() => channel.encryptRequest(new Uint8Array())).toThrow('login_expired')
  channel.dispose()
})

it('binds the signed expiry and sends no commitment, reveal or nonce fields', () => {
  const client = start()
  expect(client.hello).toEqual(vector.request)
  expect(Object.keys(client.hello)).toEqual(['publicKey'])
  expect(() => client.accept({ ...vector.response, expiresAt: vector.response.expiresAt + 1 }, vector.serverIdentity)).toThrow('signature_verification_failed')
})

it('clears a cancelled exchange and refuses to reuse it', () => {
  const client = start()
  client.dispose()
  expect(() => client.accept(vector.response, vector.serverIdentity)).toThrow('invalid_auth_response')
})
