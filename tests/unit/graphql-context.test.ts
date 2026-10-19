import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { bindGraphqlClient, gqlFetch, GqlError } from '@/lib/api/gql-client'
import { nativeGraphql } from '@/lib/api/native-client'
import { httpRequest } from '@/lib/api/http'
import { clearCurrentSession } from '@/lib/device/current'
import { initMutation } from '@/lib/api/mutation'
import { initLazyQuery } from '@/lib/api/query'
import emitter from '@/plugins/eventbus'

const target = vi.hoisted(() => ({ native: true, client: 'device-a', url: 'https://device-a.test', token: 'session-a', header: 'client-a' }))
vi.mock('@/lib/api/api', () => ({
  isNativeTarget: () => target.native,
  getApiBaseUrl: () => target.url,
  getApiHeaders: () => ({ 'c-id': target.header }),
  getLocalToken: () => target.token,
}))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => target.token, clearCurrentSession: vi.fn() }))
vi.mock('@/lib/device/client-id', () => ({ getActiveClientId: () => target.client }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => target.native }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: vi.fn() } }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: vi.fn() }))
vi.mock('@/lib/api/http', () => ({ httpRequest: vi.fn() }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: (token: string) => token }))
vi.mock('@/lib/api/time-sync', () => ({ wrapWithReplayProtection: (value: string) => value }))
vi.mock('@/lib/api/crypto', () => ({
  httpResponseAad: (_: unknown, request: unknown) => request,
  chachaEncrypt: (_: unknown, value: string) => new TextEncoder().encode(value),
  chachaDecrypt: (_: unknown, value: ArrayBuffer) => new TextDecoder().decode(value),
  arrayBufferToBitArray: (value: unknown) => value,
  bitArrayToUint8Array: (value: unknown) => value,
}))

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => { resolve = done })
  return { promise, resolve }
}

function response(data: unknown, status = 200) {
  return { status, arrayBuffer: async () => new TextEncoder().encode(JSON.stringify({ data })).buffer } as unknown as Awaited<ReturnType<typeof httpRequest>>
}

beforeEach(() => {
  Object.assign(target, { native: true, client: 'device-a', url: 'https://device-a.test', token: 'session-a', header: 'client-a' })
  vi.clearAllMocks()
  vi.stubGlobal('__IS_TAURI__', false)
  vi.stubGlobal('window', { __PLAIN_LOG__: false, location: { reload: vi.fn() } })
})
afterEach(() => { vi.unstubAllGlobals() })

describe('GraphQL request ownership', () => {
  it('still deduplicates identical requests within one connection', async () => {
    const reply = deferred<any>()
    vi.mocked(nativeGraphql).mockReturnValueOnce(reply.promise)
    const first = gqlFetch('query { app { id } }')
    const second = gqlFetch('query { app { id } }')
    expect(nativeGraphql).toHaveBeenCalledTimes(1)
    reply.resolve({ data: { app: { id: 'a' } } })
    expect(await first).toEqual(await second)
  })

  it('does not reuse an old-device request or deliver its response', async () => {
    const reply = deferred<any>()
    vi.mocked(nativeGraphql).mockReturnValueOnce(reply.promise)
    const first = gqlFetch('query { app { id } }')
    const stale = expect(first).rejects.toThrow('request_context_changed')
    target.client = 'device-b'
    vi.mocked(nativeGraphql).mockResolvedValueOnce({ data: { app: { id: 'b' } } })
    expect((await gqlFetch('query { app { id } }')).data).toEqual({ app: { id: 'b' } })
    reply.resolve({ data: { app: { id: 'a' } } })
    await stale
    expect(nativeGraphql).toHaveBeenCalledTimes(2)
  })

  it('separates sessions and pins outgoing HTTP headers', async () => {
    target.native = false
    const reply = deferred<Awaited<ReturnType<typeof httpRequest>>>()
    vi.mocked(httpRequest).mockReturnValueOnce(reply.promise)
    const first = gqlFetch('query { app { id } }')
    const stale = expect(first).rejects.toThrow('request_context_changed')
    target.token = 'session-b'
    target.header = 'client-b'
    vi.mocked(httpRequest).mockResolvedValueOnce(response({ app: { id: 'b' } }))
    expect((await gqlFetch('query { app { id } }')).data).toEqual({ app: { id: 'b' } })
    expect(vi.mocked(httpRequest).mock.calls[0]?.[1]?.headers).toEqual({ 'c-id': 'client-a' })
    expect(vi.mocked(httpRequest).mock.calls[1]?.[1]?.headers).toEqual({ 'c-id': 'client-b' })
    reply.resolve(response({ app: { id: 'a' } }))
    await stale
  })

  it('does not clear a new session for an old-session unauthorized response', async () => {
    target.native = false
    const reply = deferred<Awaited<ReturnType<typeof httpRequest>>>()
    vi.mocked(httpRequest).mockReturnValueOnce(reply.promise)
    const first = gqlFetch('query { app { id } }', undefined, { dedupe: false })
    const failure = expect(first).rejects.toThrow('unauthorized')
    target.token = 'session-b'
    reply.resolve(response(null, 401))
    await failure
    expect(clearCurrentSession).not.toHaveBeenCalled()
    expect(window.location.reload).not.toHaveBeenCalled()
  })

  it('keeps explicit bound cleanup requests on their original device', async () => {
    target.native = false
    const bound = bindGraphqlClient()
    target.url = 'https://device-b.test'
    target.token = 'session-b'
    vi.mocked(httpRequest).mockResolvedValueOnce(response({ releaseDiscovery: true }))
    expect((await bound('mutation { releaseDiscovery(token: "scope") }')).data).toEqual({ releaseDiscovery: true })
    expect(vi.mocked(httpRequest).mock.calls[0]?.[0]).toBe('https://device-a.test/graphql')
  })
})

it('marks a discarded response as cancellation rather than a displayed request failure', async () => {
  const reply = deferred<any>()
  vi.mocked(nativeGraphql).mockReturnValueOnce(reply.promise)
  const request = gqlFetch('query { app { id } }', undefined, { dedupe: false }).catch((error: unknown) => error)
  target.client = 'device-b'
  reply.resolve({ data: { app: { id: 'a' } } })
  const error = await request
  expect(error).toBeInstanceOf(GqlError)
  expect(error.cancelled).toBe(true)
})

it('does not run mutation callbacks or show a toast for a discarded response', async () => {
  const reply = deferred<any>()
  vi.mocked(nativeGraphql).mockReturnValueOnce(reply.promise)
  const mutation = initMutation({ document: 'mutation { clearAudioQueue }' })
  const applied = vi.fn()
  const failed = vi.fn()
  mutation.onDone(applied)
  mutation.onError(failed)
  const request = mutation.mutate()
  target.client = 'device-b'
  reply.resolve({ data: { clearAudioQueue: true } })
  expect(await request).toBeUndefined()
  expect(applied).not.toHaveBeenCalled()
  expect(failed).not.toHaveBeenCalled()
  expect(emitter.emit).not.toHaveBeenCalled()
  expect(mutation.loading.value).toBe(false)
})

it('does not deliver stale data or errors to query projections', async () => {
  const reply = deferred<any>()
  vi.mocked(nativeGraphql).mockReturnValueOnce(reply.promise)
  const handle = vi.fn()
  const query = initLazyQuery({ document: 'query { app { id } }', handle })
  const request = query.fetch()
  target.client = 'device-b'
  reply.resolve({ data: { app: { id: 'a' } } })
  await request
  expect(handle).not.toHaveBeenCalled()
  expect(query.result.value).toBeUndefined()
  expect(query.loading.value).toBe(false)
})
