import { afterEach, beforeEach, expect, it, vi } from 'vitest'
const state = vi.hoisted(() => ({ token: '', base: '', request: vi.fn() }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => state.token }))
vi.mock('@/lib/api/api', () => ({ getApiBaseUrl: () => state.base, getApiHeaders: () => ({ 'c-id': 'browser' }) }))
vi.mock('@/lib/api/http', () => ({ httpRequest: state.request }))
vi.mock('@/lib/api/time-sync', () => ({ wrapWithReplayProtection: (value: string) => `1700000000000|synthetic-nonce|${value}` }))
import { requestInit } from '../../src/lib/api/init'
import { chachaDecrypt, httpRequestAad } from '../../src/lib/api/crypto'
const key = new Uint8Array(32).fill(7)
beforeEach(() => {
  state.token = Buffer.from(key).toString('base64')
  state.base = 'https://fixture.test'
  state.request.mockResolvedValue({ status: 200, text: async () => JSON.stringify({ authenticated: false, passwordRequired: true, signaturePublicKey: 'fixture' }) })
})
afterEach(() => vi.clearAllMocks())
it.each(['', '/proxy/target'])('binds the replay-protected init probe to its logical endpoint through %s', async path => {
  state.base += path
  await requestInit()
  const [url, options] = state.request.mock.calls[0]
  expect(url).toBe(state.base + '/init')
  expect(options.method).toBe('POST')
  expect(chachaDecrypt(key, options.body, httpRequestAad('/init'))).toBe('1700000000000|synthetic-nonce|init')
  expect(() => chachaDecrypt(key, options.body)).toThrow()
  expect(() => chachaDecrypt(key, options.body, httpRequestAad('/graphql'))).toThrow()
})
it('keeps unauthenticated initialization available without an encrypted body', async () => {
  state.token = ''
  const result = await requestInit()
  expect(state.request.mock.calls[0][1].body).toBeUndefined()
  expect(result.data?.authenticated).toBe(false)
})
