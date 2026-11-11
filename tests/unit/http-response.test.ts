import { readFileSync } from 'node:fs'
import { createHash, randomBytes, randomUUID } from 'node:crypto'
import { expect, it, vi, afterEach } from 'vitest'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { chachaDecrypt, httpRequestAad, httpResponseAad } from '../../src/lib/api/crypto'
import { encryptedGqlPost, GqlError } from '../../src/lib/api/gql-client'
import { guestFetch } from '../../src/lib/api/guest'
import { httpRequest } from '../../src/lib/api/http'

vi.mock('@/lib/api/api', () => ({
  getApiBaseUrl: () => 'https://fixture.test',
  getApiHeaders: () => ({ 'c-id': 'fixture' }),
  getUrlToken: () => '',
  getProxyUrl: (path: string) => path,
}))
vi.mock('@/lib/api/time-sync', () => ({ wrapWithReplayProtection: (json: string) => `${Date.now()}|${randomUUID()}|${json}` }))
vi.mock('@/lib/api/http', () => ({ httpRequest: vi.fn() }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: (token: string) => Buffer.from(token, 'base64') }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => '', clearCurrentSession: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@/lib/device/client-id', () => ({ getActiveClientId: () => 'fixture' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: vi.fn() }))

afterEach(() => {
  vi.unstubAllGlobals()
  vi.resetAllMocks()
  vi.restoreAllMocks()
})

const vectors = JSON.parse(readFileSync(new URL('../../../plain-core/plain-rs/testdata/crypto/http-response-vectors.json', import.meta.url), 'utf8'))

it('matches the shared Rust response framing and rejects response substitution', () => {
  for (const vector of vectors) {
    const key = Buffer.from(vector.key, 'base64')
    const request = Buffer.from(vector.request, 'base64')
    const response = Buffer.from(vector.response, 'base64')
    const aad = httpResponseAad(request)
    expect(Buffer.from(aad).toString('hex')).toBe(vector.aadHex)
    expect(chachaDecrypt(key, response, aad)).toBe(vector.plaintext)
    expect(() => chachaDecrypt(key, response)).toThrow()
    expect(aad.length).toBe(34)
    expect(Buffer.from(aad)).toEqual(Buffer.concat([Buffer.from([0x72, 0]), createHash('sha256').update(request).digest()]))
    for (const index of [0, 24, request.length - 1]) {
      const changed = Uint8Array.from(request)
      changed[index] ^= 1
      expect(() => chachaDecrypt(key, response, httpResponseAad(changed))).toThrow()
    }
    expect(() => chachaDecrypt(key, response, httpRequestAad(vector.endpoint))).toThrow()
    const oldAad = Buffer.concat([Buffer.from(`plain-http-response-v1\0${vector.endpoint}\0`), createHash('sha256').update(request).digest()])
    expect(() => chachaDecrypt(key, response, oldAad)).toThrow()
  }
})

it.each(['/graphql', '/guest_graphql'])('the actual %s client rejects captured and legacy replies', async (endpoint) => {
  vi.stubGlobal('window', { __PLAIN_LOG__: false })
  const now = 1794424320000
  vi.spyOn(Date, 'now').mockReturnValue(now)
  const key = Buffer.alloc(32, 7)
  const requestIds: string[] = []
  const variables = { input: { text: 'synthetic|value', count: 2 } }
  let mode = 'fresh'
  let captured = new Uint8Array()
  const requests: Uint8Array[] = []
  vi.mocked(httpRequest).mockImplementation(async (url, options) => {
    expect(new URL(url).pathname).toBe(endpoint)
    const request = options!.body!
    requests.push(request.slice())
    const payload = chachaDecrypt(key, request, httpRequestAad(endpoint))
    if (endpoint === '/graphql') {
      const parsed = JSON.parse(payload)
      expect(Object.keys(parsed).sort()).toEqual(['query', 'requestId', 'variables'])
      expect(parsed.query).toBe('query { __typename }')
      expect(parsed.variables).toEqual(variables)
      expect(parsed.requestId).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/)
      expect(Number.parseInt(parsed.requestId.replaceAll('-', '').slice(0, 12), 16)).toBe(now)
      requestIds.push(parsed.requestId)
    } else {
      expect(payload.startsWith(`${now}|`)).toBe(true)
      expect(payload.slice(payload.indexOf('|', payload.indexOf('|') + 1) + 1)).toBe(JSON.stringify({ query: 'query { __typename }', variables }))
    }
    expect(() => chachaDecrypt(key, request)).toThrow()
    expect(() => chachaDecrypt(key, request, new TextEncoder().encode(`http-request\0GET\0${endpoint}`))).toThrow()
    expect(() => chachaDecrypt(key, request, httpRequestAad(endpoint === '/graphql' ? '/guest_graphql' : '/graphql'))).toThrow()
    const nonce = randomBytes(24)
    const aad = mode === 'legacy' ? undefined
      : mode === 'old-aad' ? Buffer.concat([Buffer.from(`plain-http-response-v1\0${endpoint}\0`), createHash('sha256').update(request).digest()])
      : mode === 'request-aad' ? httpRequestAad(endpoint)
      : httpResponseAad(request)
    const encrypted = xchacha20poly1305(key, nonce, aad).encrypt(new TextEncoder().encode('{"data":{"__typename":"Query"}}'))
    const bytes = new Uint8Array(24 + encrypted.length)
    bytes.set(nonce)
    bytes.set(encrypted, 24)
    const body = mode === 'replay' ? captured : bytes
    if (mode === 'fresh') captured = bytes
    return { status: 200, ok: true, arrayBuffer: async () => body.slice().buffer } as Response
  })
  const call = () => (endpoint === '/graphql' ? encryptedGqlPost('https://fixture.test/graphql', key.toString('base64'), 'query { __typename }', variables) : guestFetch('fixture', key, 'query { __typename }', variables))
  expect((await call()).data.__typename).toBe('Query')
  mode = 'replay'
  await expect(call()).rejects.toThrow()
  for (const invalid of ['legacy', 'old-aad', 'request-aad']) {
    mode = invalid
    await expect(call()).rejects.toThrow()
  }
  mode = 'fresh'
  expect((await call()).data.__typename).toBe('Query')
  expect(Buffer.from(requests[0])).not.toEqual(Buffer.from(requests[1]))
  expect(new Set(requestIds).size).toBe(requestIds.length)
  expect(httpRequest).toHaveBeenCalledTimes(6)
})

it.each(['/graphql', '/guest_graphql', '/peer_graphql', '/init', '/upload', '/upload_chunk'])('binds %s to the agreed POST request domain', endpoint => {
  expect(new TextDecoder().decode(httpRequestAad(endpoint))).toBe('http-request\0POST\0' + endpoint)
})


it.each([400, 429, 500])('preserves HTTP %s without decrypting or retrying rejected requests', async status => {
  vi.stubGlobal('window', { __PLAIN_LOG__: false })
  const response = new Response(null, { status })
  const read = vi.spyOn(response, 'arrayBuffer')
  vi.mocked(httpRequest).mockResolvedValue(response)
  await expect(encryptedGqlPost('https://fixture.test/graphql', Buffer.alloc(32, 7).toString('base64'), 'query { __typename }'))
    .rejects.toMatchObject(new GqlError('network_error', status))
  expect(read).not.toHaveBeenCalled()
  expect(httpRequest).toHaveBeenCalledTimes(1)
})
