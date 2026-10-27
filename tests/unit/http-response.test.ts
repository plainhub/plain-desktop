import { readFileSync } from 'node:fs'
import { randomBytes, randomUUID } from 'node:crypto'
import { expect, it, vi, afterEach } from 'vitest'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { chachaDecrypt, httpResponseAad } from '../../src/lib/api/crypto'
import { encryptedGqlPost } from '../../src/lib/api/gql-client'
import { guestFetch } from '../../src/lib/api/guest'
import { httpRequest } from '../../src/lib/api/http'

vi.mock('@/lib/api/api', () => ({
  getApiBaseUrl: () => 'https://fixture.test',
  getApiHeaders: () => ({ 'c-id': 'fixture' }),
  getLocalToken: () => '',
  getProxyUrl: (path: string) => path,
}))
vi.mock('@/lib/api/http', () => ({ httpRequest: vi.fn() }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: (token: string) => Buffer.from(token, 'base64') }))
vi.mock('@/lib/api/time-sync', () => ({ wrapWithReplayProtection: (json: string) => `${Date.now()}|${randomUUID()}|${json}` }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => '', clearCurrentSession: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@/lib/device/client-id', () => ({ getActiveClientId: () => 'fixture' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: vi.fn() }))

afterEach(() => {
  vi.unstubAllGlobals()
  vi.resetAllMocks()
})

const vectors = JSON.parse(readFileSync(new URL('../../../plain-core/plain-rs/testdata/crypto/http-response-vectors.json', import.meta.url), 'utf8'))

it('matches the shared Rust response framing and rejects response substitution', () => {
  for (const vector of vectors) {
    const key = Buffer.from(vector.key, 'base64')
    const request = Buffer.from(vector.request, 'base64')
    const response = Buffer.from(vector.response, 'base64')
    const aad = httpResponseAad(vector.endpoint, request)
    expect(Buffer.from(aad).toString('hex')).toBe(vector.aadHex)
    expect(chachaDecrypt(key, response, aad)).toBe(vector.plaintext)
    expect(() => chachaDecrypt(key, response)).toThrow()
    expect(() => chachaDecrypt(key, response, httpResponseAad(vector.endpoint, new Uint8Array([1])))).toThrow()
    expect(() => chachaDecrypt(key, response, httpResponseAad('/other', request))).toThrow()
  }
})

it.each(['/graphql', '/guest_graphql'])('the actual %s client rejects captured and legacy replies', async (endpoint) => {
  vi.stubGlobal('window', { __PLAIN_LOG__: false })
  const key = Buffer.alloc(32, 7)
  let mode = 'fresh'
  let captured = new Uint8Array()
  const requests: Uint8Array[] = []
  vi.mocked(httpRequest).mockImplementation(async (url, options) => {
    expect(new URL(url).pathname).toBe(endpoint)
    const request = options!.body!
    requests.push(request.slice())
    const nonce = randomBytes(24)
    const encrypted = xchacha20poly1305(key, nonce, mode === 'legacy' ? undefined : httpResponseAad(endpoint, request)).encrypt(new TextEncoder().encode('{"data":{"__typename":"Query"}}'))
    const bytes = new Uint8Array(24 + encrypted.length)
    bytes.set(nonce)
    bytes.set(encrypted, 24)
    const body = mode === 'replay' ? captured : bytes
    if (mode === 'fresh') captured = bytes
    return { status: 200, ok: true, arrayBuffer: async () => body.slice().buffer } as Response
  })
  const call = () => (endpoint === '/graphql' ? encryptedGqlPost('https://fixture.test/graphql', key.toString('base64'), 'query { __typename }') : guestFetch('fixture', key, 'query { __typename }'))
  expect((await call()).data.__typename).toBe('Query')
  mode = 'replay'
  await expect(call()).rejects.toThrow()
  mode = 'legacy'
  await expect(call()).rejects.toThrow()
  mode = 'fresh'
  expect((await call()).data.__typename).toBe('Query')
  expect(Buffer.from(requests[0])).not.toEqual(Buffer.from(requests[1]))
})
