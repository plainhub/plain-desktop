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

it('shares only concurrent init probes for the same target and session', async () => {
  let resolve!: (value: unknown) => void
  state.request.mockImplementationOnce(() => new Promise(done => { resolve = done }))
  const first = requestInit()
  expect(requestInit()).toBe(first)
  state.base = 'https://other.test'
  await requestInit()
  expect(state.request).toHaveBeenCalledTimes(2)
  resolve({ status: 200, text: async () => JSON.stringify({ httpsPort: 9443 }) })
  expect((await first).data?.httpsPort).toBe(9443)
  state.base = 'https://fixture.test'
  await requestInit()
  expect(state.request).toHaveBeenCalledTimes(3)
})

it('allows a new init request after a failed probe', async () => {
  state.request.mockRejectedValueOnce(new Error('offline'))
  await expect(requestInit()).rejects.toThrow('offline')
  await expect(requestInit()).resolves.toMatchObject({ status: 200 })
})

it('does not reuse a pending probe after authentication changes on the same target', async () => {
  let resolve!: (value: unknown) => void
  state.request.mockImplementationOnce(() => new Promise(done => { resolve = done }))
  const authenticated = requestInit()
  state.token = ''
  const unauthenticated = requestInit()
  expect(unauthenticated).not.toBe(authenticated)
  await unauthenticated
  expect(state.request.mock.calls[1][1].body).toBeUndefined()
  resolve({ status: 200, text: async () => '' })
  await authenticated
})
