import { AuthErrorCode } from '@/lib/api/auth-error-code'
import { HttpErrorCode } from '@/lib/api/http-error-code'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { GqlError } from '@/lib/api/gql-error'
import { bindGraphqlClient, gqlFetch } from '@/lib/api/gql-client'
import { nativeGraphql } from '@/lib/api/native-client'
import { httpRequest } from '@/lib/api/http'
import { getCurrentAuthToken, clearCurrentSession } from '@/lib/device/current'
import { initMutation } from '@/lib/api/mutation'
import { initLazyQuery } from '@/lib/api/query'
import emitter from '@/plugins/eventbus'

const target = vi.hoisted(() => ({ native: true, client: '706c6169-7efd-857c-91a3-7471150200cd', url: 'https://device-a.test', token: 'session-a', header: '706c6169-1295-84f7-9959-21fe29798997', revision: 1 }))
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
  Object.assign(target, { native: true, client: '706c6169-7efd-857c-91a3-7471150200cd', url: 'https://device-a.test', token: 'session-a', header: '706c6169-1295-84f7-9959-21fe29798997', revision: 1 })
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
    const stale = expect(first).rejects.toThrow('REQUEST_CONTEXT_CHANGED')
    target.client = '706c6169-02de-8bc5-8fe4-50bb3e139c1b'
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
    const stale = expect(first).rejects.toThrow('REQUEST_CONTEXT_CHANGED')
    target.token = 'session-b'
    target.header = '706c6169-91e1-8871-93a5-0b60cb96e6b8'
    vi.mocked(httpRequest).mockResolvedValueOnce(response({ app: { id: 'b' } }))
    expect((await gqlFetch('query { app { id } }')).data).toEqual({ app: { id: 'b' } })
    expect(vi.mocked(httpRequest).mock.calls[0]?.[1]?.headers).toEqual({ 'c-id': '706c6169-1295-84f7-9959-21fe29798997' })
    expect(vi.mocked(httpRequest).mock.calls[1]?.[1]?.headers).toEqual({ 'c-id': '706c6169-91e1-8871-93a5-0b60cb96e6b8' })
    reply.resolve(response({ app: { id: 'a' } }))
    await stale
  })

  it('does not clear a new session for an old-session unauthorized response', async () => {
    vi.stubGlobal('__IS_TAURI__', false)
    target.native = false
    const reply = deferred<Awaited<ReturnType<typeof httpRequest>>>()
    vi.mocked(httpRequest).mockReturnValueOnce(reply.promise)
    const first = gqlFetch('query { app { id } }', undefined, { dedupe: false })
    const failure = expect(first).rejects.toThrow('UNAUTHORIZED')
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
    expect(vi.mocked(httpRequest).mock.calls[0]?.[0]).toBe('https://device-a.test/proxy/706c6169-7efd-857c-91a3-7471150200cd/graphql')
  })
})

it('marks a discarded response as cancellation rather than a displayed request failure', async () => {
  const reply = deferred<any>()
  vi.mocked(nativeGraphql).mockReturnValueOnce(reply.promise)
  const request = gqlFetch('query { app { id } }', undefined, { dedupe: false }).catch((error: unknown) => error)
  target.client = '706c6169-02de-8bc5-8fe4-50bb3e139c1b'
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
  target.client = '706c6169-02de-8bc5-8fe4-50bb3e139c1b'
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
  target.client = '706c6169-02de-8bc5-8fe4-50bb3e139c1b'
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
  expect(httpRequest).toHaveBeenCalledExactlyOnceWith('https://device-a.test/proxy/706c6169-7efd-857c-91a3-7471150200cd/graphql',
    expect.objectContaining({ method: 'POST', headers: { 'c-id': '706c6169-1295-84f7-9959-21fe29798997' } }))
  expect(nativeGraphql).not.toHaveBeenCalled()
})

it('routes the selected Desktop peer over IPC without HTTP or credentials', async () => {
  target.native = false
  target.client = '706c6169-02de-8bc5-8fe4-50bb3e139c1b'
  vi.mocked(nativeGraphql).mockResolvedValueOnce({ data: { app: { clientId: '706c6169-02de-8bc5-8fe4-50bb3e139c1b' } } })
  await gqlFetch('query { app { clientId } }')
  expect(nativeGraphql).toHaveBeenCalledExactlyOnceWith({ query: 'query { app { clientId } }', variables: undefined }, 30000, { kind: 'PEER', peerId: '706c6169-02de-8bc5-8fe4-50bb3e139c1b' }, undefined)
  expect(httpRequest).not.toHaveBeenCalled()
  expect(getCurrentAuthToken).not.toHaveBeenCalled()
})

it('keeps bound Desktop cleanup on its original peer after changing the selected target', async () => {
  target.native = false
  target.client = '706c6169-7efd-857c-91a3-7471150200cd'
  const bound = bindGraphqlClient()
  target.client = '706c6169-02de-8bc5-8fe4-50bb3e139c1b'
  vi.mocked(nativeGraphql).mockResolvedValueOnce({ data: { stopDiscovery: true } })
  expect((await bound('mutation { stopDiscovery }')).data).toEqual({ stopDiscovery: true })
  expect(nativeGraphql).toHaveBeenCalledExactlyOnceWith({ query: 'mutation { stopDiscovery }', variables: undefined }, 30000, { kind: 'PEER', peerId: '706c6169-7efd-857c-91a3-7471150200cd' }, expect.any(Function))
  expect(httpRequest).not.toHaveBeenCalled()
})

it('does not deduplicate or deliver responses across Desktop peer changes', async () => {
  target.native = false
  const reply = deferred<any>()
  vi.mocked(nativeGraphql).mockReturnValueOnce(reply.promise)
  const stale = expect(gqlFetch('query { app { id } }')).rejects.toMatchObject({ message: 'REQUEST_CONTEXT_CHANGED', canceled: true })
  target.client = '706c6169-02de-8bc5-8fe4-50bb3e139c1b'
  vi.mocked(nativeGraphql).mockResolvedValueOnce({ data: { app: { id: 'b' } } })
  expect((await gqlFetch('query { app { id } }')).data).toEqual({ app: { id: 'b' } })
  reply.resolve({ data: { app: { id: 'a' } } })
  await stale
  expect(vi.mocked(nativeGraphql).mock.calls.map(([, , destination]) => destination)).toEqual([
    { kind: 'PEER', peerId: '706c6169-7efd-857c-91a3-7471150200cd' }, { kind: 'PEER', peerId: '706c6169-02de-8bc5-8fe4-50bb3e139c1b' },
  ])
  expect(httpRequest).not.toHaveBeenCalled()
})

it('does not clear a replacement Desktop session after an old IPC unauthorized response', async () => {
  target.native = false
  let reject!: (reason: Error) => void
  vi.mocked(nativeGraphql).mockReturnValueOnce(new Promise((_, fail) => { reject = fail }))
  const failure = expect(gqlFetch('query { prefs }')).rejects.toMatchObject({ message: 'UNAUTHORIZED', status: 401 })
  target.token = 'session-b'
  target.revision++
  reject(new Error('UNAUTHORIZED'))
  await failure
  expect(clearCurrentSession).not.toHaveBeenCalled()
  expect(window.location.reload).not.toHaveBeenCalled()
})

it('preserves remote permission failures without clearing the selected session', async () => {
  target.native = false
  vi.mocked(nativeGraphql).mockRejectedValueOnce(new Error('REMOTE_ACCESS_DISABLED'))
  await expect(gqlFetch('query { prefs }')).rejects.toMatchObject({ message: 'REMOTE_ACCESS_DISABLED', status: 403 })
  expect(clearCurrentSession).not.toHaveBeenCalled()
})

it('waits for current Desktop remote-session cleanup before reloading', async () => {
  vi.stubGlobal('__IS_TAURI__', true)
  target.native = false
  const cleared = deferred<void>()
  vi.mocked(clearCurrentSession).mockReturnValueOnce(cleared.promise)
  vi.mocked(nativeGraphql).mockRejectedValueOnce(new Error('UNAUTHORIZED'))
  const failure = expect(gqlFetch('query { prefs }')).rejects.toThrow('UNAUTHORIZED')
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
  target.client = '706c6169-8ed4-8fb1-b534-79c532096b42'
  vi.mocked(nativeGraphql).mockResolvedValueOnce({ data: { result: true } })
  await core.set('locale', 'ja')
  expect(nativeGraphql).toHaveBeenCalledTimes(2)
  expect(vi.mocked(nativeGraphql).mock.calls.map(([, , destination]) => destination)).toEqual([{ kind: 'LOCAL' }, { kind: 'PEER', peerId: '706c6169-8ed4-8fb1-b534-79c532096b42' }])
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
  const handle = vi.fn()
  let query!: ReturnType<typeof initLazyQuery>
  const app = renderer.createApp({ setup() { query = initLazyQuery({ document: 'query Page($offset: Int!) { noteCount }', handle }); return () => null } })
  app.mount({})
  const requests = Array.from({ length: 300 }, (_, offset) => query.fetch({ offset }))
  try {
    expect(vi.mocked(httpRequest)).toHaveBeenCalledTimes(300)
    app.unmount()
    expect(signals.every(signal => !signal.aborted)).toBe(true)
    replies.forEach(reply => reply.resolve(response({ noteCount: 7 })))
    await Promise.all(requests)
    expect(query.loading.value).toBe(false)
    expect(handle).not.toHaveBeenCalled()
    expect(query.result.value).toBeUndefined()
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

it('rejects anonymous GraphQL without sending a request or reloading the page', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  target.native = false
  target.token = ''
  vi.mocked(httpRequest).mockResolvedValueOnce(response(null, 401))

  await expect(gqlFetch('query { app { id } }')).rejects.toMatchObject({ message: 'UNAUTHORIZED', status: 401 })
  expect(httpRequest).not.toHaveBeenCalled()
  expect(clearCurrentSession).not.toHaveBeenCalled()
  expect(window.location.reload).not.toHaveBeenCalled()
})


it.each([false, true].flatMap(returned => [false, true].map(failed => ({ returned, failed }))))('ignores a previous activation reply (return=$returned, failed=$failed)', async ({ returned, failed }) => {
  const { h, KeepAlive, nextTick, ref } = await import('vue')
  const { renderView } = await import('../helpers/vue-node-renderer')
  const old = deferred<any>()
  const current = deferred<any>()
  vi.mocked(nativeGraphql).mockReturnValueOnce(old.promise)
  if (returned) vi.mocked(nativeGraphql).mockReturnValueOnce(current.promise)
  const handle = vi.fn()
  let query!: ReturnType<typeof initLazyQuery>
  const Page = { setup() {
    query = initLazyQuery({ document: 'query Notes { noteCount }', handle })
    return () => h('section')
  } }
  const visible = ref(true)
  const view = renderView({ setup: () => () => h(KeepAlive, null, { default: () => visible.value ? h(Page) : h('aside') }) })
  const first = query.fetch({ query: 'text:Original' })
  try {
    visible.value = false
    await nextTick()
    if (returned) {
      visible.value = true
      await nextTick()
      const second = query.fetch({ query: 'text:Returned' })
      current.resolve({ data: { noteCount: 2 } })
      await second
      expect(query.loading.value).toBe(false)
    }
    old.resolve(failed ? { data: null, errors: [{ message: 'Synthetic old search failure' }] } : { data: { noteCount: 1 } })
    await first
    expect(handle.mock.calls.map(([data]) => data)).toEqual(returned ? [{ noteCount: 2 }] : [])
    expect(query.result.value).toEqual(returned ? { noteCount: 2 } : undefined)
  } finally {
    old.resolve({ data: { noteCount: 1 } })
    await first
    view.unmount()
  }
})

it('uses the origin key for managed resources without requesting the target credentials', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  target.native = false
  const { appGQL } = await import('@/lib/api/query')
  vi.mocked(httpRequest).mockReset()
    .mockResolvedValueOnce(response({ app: { clientId: 'managed-device', deviceName: 'PlainApp Desktop' } }))
    .mockResolvedValueOnce(response({ app: { urlToken: 'origin-only-resource-key' } }))
  const result = await gqlFetch(appGQL)
  expect(result.data.app).toEqual({ clientId: 'managed-device', deviceName: 'PlainApp Desktop', urlToken: 'origin-only-resource-key' })
  const [[peerUrl, peerOptions], [originUrl, originOptions]] = vi.mocked(httpRequest).mock.calls
  expect(peerUrl).toBe(`https://device-a.test/proxy/${target.client}/graphql`)
  expect(JSON.parse(new TextDecoder().decode(peerOptions?.body!)).query).not.toContain('urlToken')
  expect(originUrl).toBe('https://device-a.test/graphql')
  expect(JSON.parse(new TextDecoder().decode(originOptions?.body!)).query).toBe('{ app { urlToken } }')
  expect(peerOptions?.headers?.['c-id']).toBe(originOptions?.headers?.['c-id'])
})

it('does not sign out the browser origin when its managed target rejects the data request', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  target.native = false
  vi.mocked(httpRequest).mockReset().mockResolvedValueOnce(response({}, 401))
  await expect(gqlFetch('{ app { deviceName } }')).rejects.toMatchObject({ status: 401 })
  expect(clearCurrentSession).not.toHaveBeenCalled()
  expect(window.location.reload).not.toHaveBeenCalled()
})

it('checks the gateway once for concurrent unauthorized selected-peer requests and keeps its session', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  target.native = false
  const { managedPeerFailure } = await import('@/lib/api/gql-client')
  const probe = deferred<Awaited<ReturnType<typeof httpRequest>>>()
  vi.mocked(httpRequest).mockImplementation(url => String(url).includes('/proxy/')
    ? Promise.resolve(response(null, 401)) : probe.promise)
  const attempts = Promise.allSettled([gqlFetch('{ app { deviceName } }'), gqlFetch('{ peers { id } }')])
  await vi.waitFor(() => expect(httpRequest).toHaveBeenCalledTimes(3))
  expect(vi.mocked(httpRequest).mock.calls[2]?.[0]).toBe('https://device-a.test/graphql')
  expect(managedPeerFailure.value).toBeUndefined()
  probe.resolve(response({ app: { deviceName: 'Smartbox' }, peers: [{ id: target.client, name: 'Desktop' }] }))
  await attempts
  await vi.waitFor(() => expect(managedPeerFailure.value).toMatchObject({ originName: 'Smartbox', peerName: 'Desktop' }))
  expect(clearCurrentSession).not.toHaveBeenCalled()
  expect(window.location.reload).not.toHaveBeenCalled()
})

it('uses normal origin sign-in recovery when the origin also responds unauthorized', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  Object.assign(target, { native: false, client: '70f5a619-02de-8bc5-8fe4-50bb3e139c1b' })
  const { managedPeerFailure } = await import('@/lib/api/gql-client')
  const previousFailure = managedPeerFailure.value
  vi.mocked(httpRequest).mockResolvedValue(response(null, 401))
  await expect(gqlFetch('{ app { deviceName } }')).rejects.toThrow('UNAUTHORIZED')
  await vi.waitFor(() => expect(window.location.reload).toHaveBeenCalledOnce())
  expect(clearCurrentSession).toHaveBeenCalledOnce()
  expect(httpRequest).toHaveBeenCalledTimes(2)
  expect(managedPeerFailure.value).toBe(previousFailure)
})

it('does not show the pairing dialog when the selected peer changes during the origin check', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  Object.assign(target, { native: false, client: '70f5a619-03de-8bc5-8fe4-50bb3e139c1b' })
  const { managedPeerFailure } = await import('@/lib/api/gql-client')
  const previousFailure = managedPeerFailure.value
  const probe = deferred<Awaited<ReturnType<typeof httpRequest>>>()
  vi.mocked(httpRequest).mockImplementation(url => String(url).includes('/proxy/')
    ? Promise.resolve(response(null, 401)) : probe.promise)
  await expect(gqlFetch('{ app { deviceName } }')).rejects.toThrow('UNAUTHORIZED')
  await vi.waitFor(() => expect(httpRequest).toHaveBeenCalledTimes(2))
  target.client = '70f5a619-04de-8bc5-8fe4-50bb3e139c1b'
  probe.resolve(response({ app: { deviceName: 'Smartbox' }, peers: [] }))
  await new Promise(resolve => setTimeout(resolve, 0))
  expect(managedPeerFailure.value).toBe(previousFailure)
  expect(clearCurrentSession).not.toHaveBeenCalled()
  expect(window.location.reload).not.toHaveBeenCalled()
})

it('offers recovery for a rejected selected-peer route without rechecking the gateway login', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  Object.assign(target, { native: false, client: '70f5a619-05de-8bc5-8fe4-50bb3e139c1b' })
  const { managedPeerFailure } = await import('@/lib/api/gql-client')
  vi.mocked(httpRequest).mockResolvedValue(response(null, 502))
  await expect(gqlFetch('{ app { deviceName } }')).rejects.toMatchObject({ status: 502 })
  expect(managedPeerFailure.value).toEqual({ peerId: target.client })
  expect(httpRequest).toHaveBeenCalledOnce()
  expect(clearCurrentSession).not.toHaveBeenCalled()
  expect(window.location.reload).not.toHaveBeenCalled()
})

it('leaves selected-peer 500s and origin 502s to ordinary handling', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  Object.assign(target, { native: false, client: '70f5a619-06de-8bc5-8fe4-50bb3e139c1b' })
  const { managedPeerFailure } = await import('@/lib/api/gql-client')
  const previousFailure = managedPeerFailure.value
  vi.mocked(httpRequest).mockResolvedValueOnce(response(null, 500)).mockResolvedValueOnce(response(null, 502))
  await expect(gqlFetch('{ app { deviceName } }')).rejects.toMatchObject({ status: 500 })
  await expect(gqlFetch('{ app { deviceName } }', undefined, { origin: true })).rejects.toMatchObject({ status: 502 })
  expect(managedPeerFailure.value).toBe(previousFailure)
  expect(httpRequest).toHaveBeenCalledTimes(2)
  expect(clearCurrentSession).not.toHaveBeenCalled()
})

it('preserves structured mutation error codes independently of diagnostic text', async () => {
  const mutation = initMutation({ document: 'mutation { logout }' })
  const failed = vi.fn()
  mutation.onError(failed)
  vi.mocked(nativeGraphql).mockResolvedValueOnce({ errors: [{ message: 'approval diagnostic', extensions: { code: 'REJECTED' } }] })
  await mutation.mutate()
  expect(failed).toHaveBeenCalledWith(expect.objectContaining({ code: AuthErrorCode.Rejected, message: 'approval diagnostic' }))
  expect(emitter.emit).not.toHaveBeenCalled()
})

it('delivers transport failures to the caller without a global toast', async () => {
  const failed = vi.fn()
  const mutation = initMutation({ document: 'mutation { logout }' })
  mutation.onError(failed)
  vi.mocked(nativeGraphql).mockRejectedValueOnce(new Error('transport unavailable'))
  expect(await mutation.mutate()).toBeUndefined()
  expect(failed).toHaveBeenCalledTimes(1)
  expect(emitter.emit).not.toHaveBeenCalled()
  expect(mutation.loading.value).toBe(false)
})

it('keeps network diagnostic text from becoming a business failure', async () => {
  const { encryptedGqlPost } = await import('@/lib/api/gql-client')
  vi.mocked(httpRequest).mockRejectedValueOnce(new Error('INVALID_PASSWORD'))
  await expect(encryptedGqlPost('https://fixture.test/graphql', 'session-a', '{ __typename }'))
    .rejects.toMatchObject({ code: HttpErrorCode.NetworkError, message: 'INVALID_PASSWORD' })
})
