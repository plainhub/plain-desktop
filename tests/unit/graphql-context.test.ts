import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { bindGraphqlClient, gqlFetch, GqlError } from '@/lib/api/gql-client'
import { nativeGraphql } from '@/lib/api/native-client'
import { httpRequest } from '@/lib/api/http'
import { getCurrentAuthToken, clearCurrentSession } from '@/lib/device/current'
import { initMutation } from '@/lib/api/mutation'
import { initLazyQuery } from '@/lib/api/query'
import emitter from '@/plugins/eventbus'

const target = vi.hoisted(() => ({ native: true, client: 'device-a', url: 'https://device-a.test', token: 'session-a', header: 'client-a', revision: 1 }))
vi.mock('@/lib/api/api', () => ({
  getApiBaseUrl: () => target.url,
  getApiHeaders: () => ({ 'c-id': target.header }),
  getUrlToken: () => target.token,
}))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: vi.fn(() => target.token), clearCurrentSession: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@/lib/device/login-peers', () => ({ getLoginPeerRevision: () => target.revision }))
vi.mock('@/lib/device/client-id', () => ({ getActiveClientId: () => target.client, getRemoteClientId: () => target.native ? '' : target.client }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => target.native }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: vi.fn() } }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: vi.fn() }))
vi.mock('@/lib/api/http', () => ({ httpRequest: vi.fn() }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: (token: string) => token }))
vi.mock('@/lib/api/time-sync', () => ({ wrapWithReplayProtection: (value: string) => value }))
vi.mock('@/lib/api/crypto', async original => ({
  ...(await original<typeof import('@/lib/api/crypto')>()),

  chachaEncrypt: (_: unknown, value: string) => new TextEncoder().encode(value),
  decryptGraphqlResponse: (_: unknown, _request: unknown, value: ArrayBuffer) => new TextDecoder().decode(value),
  arrayBufferToBitArray: (value: unknown) => value,
  bitArrayToUint8Array: (value: unknown) => value,
}))

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => { resolve = done })
  return { promise, resolve }
}

function response(data: unknown, status = 200) {
  return { status, ok: status >= 200 && status < 300, arrayBuffer: async () => new TextEncoder().encode(JSON.stringify({ data })).buffer } as unknown as Awaited<ReturnType<typeof httpRequest>>
}

beforeEach(() => {
  Object.assign(target, { native: true, client: 'device-a', url: 'https://device-a.test', token: 'session-a', header: 'client-a', revision: 1 })
  vi.clearAllMocks()
  vi.stubGlobal('__IS_TAURI__', true)
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
    vi.stubGlobal('__IS_TAURI__', false)
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
    vi.stubGlobal('__IS_TAURI__', false)
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
    vi.stubGlobal('__IS_TAURI__', false)
    target.native = false
    const bound = bindGraphqlClient()
    target.url = 'https://device-b.test'
    target.token = 'session-b'
    vi.mocked(httpRequest).mockResolvedValueOnce(response({ stopDiscovery: true }))
    expect((await bound('mutation { stopDiscovery }')).data).toEqual({ stopDiscovery: true })
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
  expect(error.canceled).toBe(true)
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

it('keeps browser remote requests on encrypted HTTPS', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  target.native = false
  vi.mocked(httpRequest).mockResolvedValueOnce(response({ prefs: { locale: 'fr' } }))
  expect((await gqlFetch('query { prefs }')).data).toEqual({ prefs: { locale: 'fr' } })
  expect(httpRequest).toHaveBeenCalledExactlyOnceWith('https://device-a.test/graphql',
    expect.objectContaining({ method: 'POST', headers: { 'c-id': 'client-a' } }))
  expect(nativeGraphql).not.toHaveBeenCalled()
})

it('routes the selected Desktop peer over IPC without HTTP or credentials', async () => {
  target.native = false
  target.client = 'device-b'
  vi.mocked(nativeGraphql).mockResolvedValueOnce({ data: { app: { clientId: 'device-b' } } })
  await gqlFetch('query { app { clientId } }')
  expect(nativeGraphql).toHaveBeenCalledExactlyOnceWith({ query: 'query { app { clientId } }', variables: undefined }, 30000, { kind: 'peer', peerId: 'device-b' }, undefined)
  expect(httpRequest).not.toHaveBeenCalled()
  expect(getCurrentAuthToken).not.toHaveBeenCalled()
})

it('keeps bound Desktop cleanup on its original peer after changing the selected target', async () => {
  target.native = false
  target.client = 'device-a'
  const bound = bindGraphqlClient()
  target.client = 'device-b'
  vi.mocked(nativeGraphql).mockResolvedValueOnce({ data: { stopDiscovery: true } })
  expect((await bound('mutation { stopDiscovery }')).data).toEqual({ stopDiscovery: true })
  expect(nativeGraphql).toHaveBeenCalledExactlyOnceWith({ query: 'mutation { stopDiscovery }', variables: undefined }, 30000, { kind: 'peer', peerId: 'device-a' }, expect.any(Function))
  expect(httpRequest).not.toHaveBeenCalled()
})

it('does not deduplicate or deliver responses across Desktop peer changes', async () => {
  target.native = false
  const reply = deferred<any>()
  vi.mocked(nativeGraphql).mockReturnValueOnce(reply.promise)
  const stale = expect(gqlFetch('query { app { id } }')).rejects.toMatchObject({ message: 'request_context_changed', canceled: true })
  target.client = 'device-b'
  vi.mocked(nativeGraphql).mockResolvedValueOnce({ data: { app: { id: 'b' } } })
  expect((await gqlFetch('query { app { id } }')).data).toEqual({ app: { id: 'b' } })
  reply.resolve({ data: { app: { id: 'a' } } })
  await stale
  expect(vi.mocked(nativeGraphql).mock.calls.map(([, , destination]) => destination)).toEqual([
    { kind: 'peer', peerId: 'device-a' }, { kind: 'peer', peerId: 'device-b' },
  ])
  expect(httpRequest).not.toHaveBeenCalled()
})

it('does not clear a replacement Desktop session after an old IPC unauthorized response', async () => {
  target.native = false
  let reject!: (reason: Error) => void
  vi.mocked(nativeGraphql).mockReturnValueOnce(new Promise((_, fail) => { reject = fail }))
  const failure = expect(gqlFetch('query { prefs }')).rejects.toMatchObject({ message: 'unauthorized', status: 401 })
  target.token = 'session-b'
  target.revision++
  reject(new Error('unauthorized'))
  await failure
  expect(clearCurrentSession).not.toHaveBeenCalled()
  expect(window.location.reload).not.toHaveBeenCalled()
})

it('preserves remote permission failures without clearing the selected session', async () => {
  target.native = false
  vi.mocked(nativeGraphql).mockRejectedValueOnce(new Error('remote_access_disabled'))
  await expect(gqlFetch('query { prefs }')).rejects.toMatchObject({ message: 'remote_access_disabled', status: 403 })
  expect(clearCurrentSession).not.toHaveBeenCalled()
})

it('waits for current Desktop remote-session cleanup before reloading', async () => {
  vi.stubGlobal('__IS_TAURI__', true)
  target.native = false
  const cleared = deferred<void>()
  vi.mocked(clearCurrentSession).mockReturnValueOnce(cleared.promise)
  vi.mocked(nativeGraphql).mockRejectedValueOnce(new Error('unauthorized'))
  const failure = expect(gqlFetch('query { prefs }')).rejects.toThrow('unauthorized')
  await vi.waitFor(() => expect(clearCurrentSession).toHaveBeenCalledTimes(1))
  expect(window.location.reload).not.toHaveBeenCalled()
  cleared.resolve()
  await failure
  expect(window.location.reload).toHaveBeenCalledTimes(1)
})

it('routes Core preference writes to the selected local or remote target', async () => {
  vi.stubGlobal('__IS_TAURI__', true)
  const core = await import('@/lib/prefs/core')
  vi.mocked(nativeGraphql).mockResolvedValueOnce({ data: { result: true } })
  await core.set('locale', 'fr')
  expect(nativeGraphql).toHaveBeenCalledTimes(1)
  expect(httpRequest).not.toHaveBeenCalled()
  target.native = false
  target.client = 'remote'
  vi.mocked(nativeGraphql).mockResolvedValueOnce({ data: { result: true } })
  await core.set('locale', 'ja')
  expect(nativeGraphql).toHaveBeenCalledTimes(2)
  expect(vi.mocked(nativeGraphql).mock.calls.map(([, , destination]) => destination)).toEqual([{ kind: 'local' }, { kind: 'peer', peerId: 'remote' }])
  expect(httpRequest).not.toHaveBeenCalled()
  expect(core.get('locale', '')).toBe('ja')
})

it('keeps 300 in-flight queries running when their page unmounts', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  const { createRenderer } = await import('vue')
  target.native = false
  const replies: Array<ReturnType<typeof deferred<any>>> = []
  const signals: AbortSignal[] = []
  vi.mocked(httpRequest).mockImplementation((_url, options) => {
    const reply = deferred<any>()
    replies.push(reply)
    signals.push(options!.signal!)
    return reply.promise
  })
  const renderer = createRenderer<any, any>({
    insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
    setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
  })
  let query!: ReturnType<typeof initLazyQuery>
  const app = renderer.createApp({ setup() { query = initLazyQuery({ document: 'query Page($offset: Int!) { noteCount }', handle: vi.fn() }); return () => null } })
  app.mount({})
  const requests = Array.from({ length: 300 }, (_, offset) => query.fetch({ offset }))
  try {
    expect(vi.mocked(httpRequest)).toHaveBeenCalledTimes(300)
    app.unmount()
    expect(signals.every(signal => !signal.aborted)).toBe(true)
    replies.forEach(reply => reply.resolve(response({ noteCount: 7 })))
    await Promise.all(requests)
    expect(query.loading.value).toBe(false)
  } finally {
    app.unmount()
    replies.forEach(reply => reply.resolve(response({ noteCount: 7 })))
    await Promise.all(requests)
  }
})

it('replaces a pending media snapshot after a library change without restoring the old rows', async () => {
  const old = deferred<any>()
  const current = deferred<any>()
  vi.mocked(nativeGraphql).mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise)
  const handle = vi.fn()
  const query = initLazyQuery({ document: 'query { audios { id } }', handle })
  const first = query.fetch()
  const refresh = query.fetch(undefined, { force: true, latest: true })
  expect(nativeGraphql).toHaveBeenCalledTimes(2)
  current.resolve({ data: { audios: [{ id: 'preserved' }] } })
  await refresh
  old.resolve({ data: { audios: [{ id: 'preserved' }, { id: 'deleted' }] } })
  await first
  expect(handle).toHaveBeenCalledExactlyOnceWith({ audios: [{ id: 'preserved' }] }, '', expect.any(Object))
  expect(query.result.value).toEqual({ audios: [{ id: 'preserved' }] })
})
