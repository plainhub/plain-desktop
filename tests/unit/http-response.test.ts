import { readFileSync } from 'node:fs'
import { createHash, randomBytes } from 'node:crypto'
import { expect, it, vi, afterEach } from 'vitest'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { chachaDecrypt, chachaDecryptBytes, httpRequestAad, chachaEncrypt, decryptGraphqlResponse } from '../../src/lib/api/crypto'
import { encryptedGqlPost, GqlError } from '../../src/lib/api/gql-client'
import { guestFetch } from '../../src/lib/api/guest'
import { httpRequest } from '../../src/lib/api/http'

vi.mock('@/lib/api/api', () => ({
  getApiBaseUrl: () => 'https://fixture.test',
  getApiHeaders: () => ({ 'c-id': 'fixture' }),
  getUrlToken: () => '',
  getProxyUrl: (path: string) => path,
}))
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
    const binding = Buffer.concat([Buffer.from([0]), createHash('sha256').update(request).digest()])
    expect(binding.toString('hex')).toBe(vector.bindingHex)
    expect(decryptGraphqlResponse(key, request, response)).toBe(vector.plaintext)
    expect(Buffer.from(chachaDecryptBytes(key, response))).toEqual(Buffer.concat([binding, Buffer.from(vector.plaintext)]))
    const requestPlain = chachaDecryptBytes(key, request)
    expect(new TextDecoder().decode(requestPlain)).toBe(vector.requestPlaintext)
    for (const index of [0, 24, request.length - 1]) {
      const changed = Uint8Array.from(request)
      changed[index] ^= 1
      expect(() => decryptGraphqlResponse(key, changed, response)).toThrow()
    }
    expect(() => decryptGraphqlResponse(key, request, request)).toThrow()
    expect(() => decryptGraphqlResponse(Buffer.alloc(32, 9), request, response)).toThrow()
    for (let length = 0; length < response.length; length++) {
      expect(() => decryptGraphqlResponse(key, request, response.subarray(0, length))).toThrow()
    }
    const tampered = Buffer.from(response)
    tampered[tampered.length - 1] ^= 1
    expect(() => decryptGraphqlResponse(key, request, tampered)).toThrow()
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
    const requestPlain = chachaDecryptBytes(key, request)
    const payload = new TextDecoder().decode(requestPlain)
    const parsed = JSON.parse(payload)
    expect(Object.keys(parsed).sort()).toEqual(['query', 'requestId', 'variables'])
    expect(parsed.query).toBe('query { __typename }')
    expect(parsed.variables).toEqual(variables)
    expect(parsed.requestId).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/)
    expect(Number.parseInt(parsed.requestId.replaceAll('-', '').slice(0, 12), 16)).toBe(now)
    requestIds.push(parsed.requestId)
    expect(() => chachaDecrypt(key, request, httpRequestAad(endpoint))).toThrow()
    const nonce = randomBytes(24)
    const binding = Buffer.concat([Buffer.from([0]), createHash('sha256').update(request).digest()])
    const response = Buffer.from('{"data":{"__typename":"Query"}}')
    const aad = mode === 'old-aad' ? Buffer.concat([Buffer.from('r\0'), createHash('sha256').update(request).digest()]) : undefined
    const plain = mode === 'legacy' || mode === 'old-aad' ? response
      : mode === 'request-aad' ? Buffer.concat([Buffer.from([1]), response])
      : Buffer.concat([binding, response])
    const encrypted = xchacha20poly1305(key, nonce, aad).encrypt(plain)
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
  expect(requestIds).toHaveLength(6)
  expect(new Set(requestIds).size).toBe(requestIds.length)
  expect(httpRequest).toHaveBeenCalledTimes(6)
})

it.each(['/init', '/upload', '/upload_chunk'])('binds %s to the agreed POST request domain', endpoint => {
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

it('encrypts the exact GraphQL payload without metadata or AAD', () => {
  const key = new Uint8Array(32).fill(7)
  const payload = '{"query":"query { __typename }"}'
  expect(chachaDecrypt(key, chachaEncrypt(key, payload))).toBe(payload)
})

it.each([false, true])('binds concurrent replies to their own requests (swapped=%s)', async swapped => {
  vi.stubGlobal('window', { __PLAIN_LOG__: false })
  const key = Buffer.alloc(32, 7)
  const pending: { request: Uint8Array; resolve: (value: Response) => void }[] = []
  vi.mocked(httpRequest).mockImplementation((_url, options) => new Promise(resolve => {
    pending.push({ request: options!.body!.slice(), resolve })
  }))
  const calls = [1, 2].map(id => encryptedGqlPost('https://fixture.test/graphql', key.toString('base64'), 'query Read($id: Int!) { value(id: $id) }', { id }))
  const result = Promise.allSettled(calls)
  expect(pending).toHaveLength(2)
  for (const index of [1, 0]) {
    const request = pending[swapped ? 1 - index : index].request
    const body = Buffer.concat([Buffer.from([0]), createHash('sha256').update(request).digest(), Buffer.from(JSON.stringify({ data: { id: index + 1 } }))])
    const nonce = randomBytes(24)
    const frame = Buffer.concat([nonce, xchacha20poly1305(key, nonce).encrypt(body)])
    pending[index].resolve({ status: 200, ok: true, arrayBuffer: async () => frame.slice().buffer.slice(frame.byteOffset, frame.byteOffset + frame.byteLength) } as Response)
  }
  const values = await result
  if (swapped) expect(values.map(value => value.status)).toEqual(['rejected', 'rejected'])
  else expect(values).toEqual([{ status: 'fulfilled', value: { data: { id: 1 } } }, { status: 'fulfilled', value: { data: { id: 2 } } }])
})
