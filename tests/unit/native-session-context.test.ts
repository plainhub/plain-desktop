import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { computed } from 'vue'

const selection = vi.hoisted(() => ({ peer: 'peer' }))
const io = vi.hoisted(() => ({ native: vi.fn(), token: vi.fn(), clear: vi.fn(), http: vi.fn(), invoke: vi.fn(), scope: vi.fn(), merge: vi.fn() }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: io.native, nativeViewScope: io.scope }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: io.invoke }))
vi.mock('@/lib/api/query', () => ({ uploadedChunksGQL: 'chunks' }))
vi.mock('@/lib/api/mutation', () => ({ deleteChunksGQL: 'delete' }))
vi.mock('@/lib/upload/merge-async', () => ({ requestMerge: io.merge }))
vi.mock('@/lib/api/http', () => ({ httpRequest: io.http }))
vi.mock('@/lib/api/api', () => ({ getApiBaseUrl: () => 'https://unused.test', proxyUrlFor: (base: string, path: string) => base + path, getApiHeaders: () => ({ 'c-id': 'self' }) }))
vi.mock('@/lib/api/crypto', () => ({}))
vi.mock('@/lib/api/file', () => ({}))
vi.mock('@/lib/api/time-sync', () => ({}))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/device/client-id', () => ({ getDesktopClientId: () => 'self', getActiveClientId: () => selection.peer, getRemoteClientId: () => selection.peer }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: io.token, clearCurrentSession: io.clear }))

import { getGraphqlContextKey, gqlFetch } from '@/lib/api/gql-client'
import { getLoginPeerRevision, loginPeers, preloadLoginPeers, type LoginPeer } from '@/lib/device/login-peers'

let snapshot: LoginPeer[]
const original = { id: 'peer', name: 'Device', token: 'original-secret', publicKey: 'original-key', createdAt: '', updatedAt: '', online: false } as LoginPeer
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail })
  return { promise, resolve, reject }
}
beforeEach(() => {
  vi.resetAllMocks()
  selection.peer = 'peer'
  vi.stubGlobal('__IS_TAURI__', true)
  vi.stubGlobal('window', Object.assign(new EventTarget(), { __PLAIN_LOG__: false, location: { reload: vi.fn() } }))
  io.scope.mockResolvedValue('document')
  loginPeers.value = []
  snapshot = [{ ...original }]
  io.token.mockImplementation(() => { throw new Error('Desktop context must not read a session token') })
  io.native.mockImplementation(async ({ query }) => query.startsWith('query { peers') ? { data: { peers: snapshot } } : { data: { result: 'current' } })
})
afterEach(() => vi.unstubAllGlobals())

it.each(['token', 'publicKey', 'remove-readd'])('invalidates and separates same-peer native requests after %s changes', async change => {
  await preloadLoginPeers()
  const key = computed(() => getGraphqlContextKey())
  const beforeKey = key.value
  const beforeRevision = getLoginPeerRevision('peer')
  const pending = deferred<any>()
  io.native.mockImplementationOnce(() => pending.promise)
  const stale = expect(gqlFetch('query { result }')).rejects.toMatchObject({ message: 'request_context_changed', canceled: true })
  if (change === 'remove-readd') {
    snapshot = []
    await preloadLoginPeers()
    expect(getLoginPeerRevision('peer')).toBe(beforeRevision + 1)
    snapshot = [{ ...original }]
  } else snapshot = [{ ...original, [change]: 'replacement-value' }]
  await preloadLoginPeers()
  expect(getLoginPeerRevision('peer')).toBe(beforeRevision + (change === 'remove-readd' ? 2 : 1))
  expect(key.value).not.toBe(beforeKey)
  expect(await gqlFetch('query { result }')).toEqual({ data: { result: 'current' } })
  pending.resolve({ data: { result: 'stale' } })
  await stale
  expect(io.native.mock.calls.filter(([body]) => body.query === 'query { result }')).toHaveLength(2)
  expect(key.value).not.toContain(original.token)
  expect(key.value).not.toContain(original.publicKey)
  expect(key.value).not.toContain('replacement-value')
  expect(io.token).not.toHaveBeenCalled()
  expect(io.http).not.toHaveBeenCalled()
})

it('keeps identical and metadata-only refreshes on the same revision and pending request', async () => {
  await preloadLoginPeers()
  const before = getLoginPeerRevision('peer')
  const pending = deferred<any>()
  io.native.mockImplementationOnce(() => pending.promise)
  const first = gqlFetch('query { result }')
  await preloadLoginPeers()
  snapshot = [{ ...original, name: 'Renamed', online: true, updatedAt: 'later' }]
  await preloadLoginPeers()
  const second = gqlFetch('query { result }')
  expect(getLoginPeerRevision('peer')).toBe(before)
  expect(io.native.mock.calls.filter(([body]) => body.query === 'query { result }')).toHaveLength(1)
  pending.resolve({ data: { result: 'kept' } })
  expect(await first).toEqual(await second)
  expect(io.token).not.toHaveBeenCalled()
})

it('does not invalidate one peer when another peer authenticates', async () => {
  await preloadLoginPeers()
  const before = getLoginPeerRevision('peer')
  const key = getGraphqlContextKey()
  snapshot = [original, { ...original, id: 'other', token: 'other-secret' }]
  await preloadLoginPeers()
  expect(getLoginPeerRevision('peer')).toBe(before)
  expect(getLoginPeerRevision('other')).toBeGreaterThan(0)
  expect(getGraphqlContextKey()).toBe(key)
})

it('does not log out a replacement same-peer session for an old unauthorized response', async () => {
  await preloadLoginPeers()
  const pending = deferred<any>()
  io.native.mockImplementationOnce(() => pending.promise)
  const failure = expect(gqlFetch('query { result }')).rejects.toMatchObject({ message: 'unauthorized', status: 401 })
  snapshot = [{ ...original, token: 'replacement-secret' }]
  await preloadLoginPeers()
  pending.reject(new Error('unauthorized'))
  await failure
  expect(io.clear).not.toHaveBeenCalled()
  expect(window.location.reload).not.toHaveBeenCalled()
  expect(io.token).not.toHaveBeenCalled()
})


it('serializes the same physical chunks across relogin and keeps old queued cleanup away from new bytes', async () => {
  const queue = await import('@/lib/upload/upload-queue')
  const { uploadChunkOwner, getUploadContext } = await import('@/lib/upload/context')
  await preloadLoginPeers()
  const firstWrite = deferred<void>()
  const oldWrite = deferred<void>()
  const releaseAbort = deferred<void>()
  const saved = new Map<string, Uint8Array>()
  const handles = new Map<string, { fileId: string; bytes: Uint8Array[]; size: number }>()
  io.native.mockImplementation(async ({ query }) => query.startsWith('query { peers') ? { data: { peers: snapshot } } : query === 'chunks' ? { data: { uploadedChunks: [] } } : { data: { deleteChunks: true } })
  io.invoke.mockImplementation(async (command, args, options) => {
    if (command === 'native_upload_chunk_open') {
      const handle = `handle-${handles.size}`
      handles.set(handle, { ...args, bytes: [] })
      return handle
    }
    if (command === 'native_upload_chunk_write') {
      const handle = options.headers['x-plain-upload-handle']
      if (handle === 'handle-0') { firstWrite.resolve(); await oldWrite.promise }
      handles.get(handle)!.bytes.push(args)
    }
    if (command === 'native_upload_chunk_abort') {
      oldWrite.reject(new Error('aborted'))
      await releaseAbort.promise
    }
    if (command === 'native_upload_chunk_finish') {
      const entry = handles.get(args.handle)!
      saved.set(entry.fileId, new Uint8Array(Buffer.concat(entry.bytes)))
      return entry.size
    }
  })
  const file = new File(['new-session-bytes'], 'shared.bin')
  const makeItem = (id: string): import('@/stores/temp').IUploadItem => ({ id, batchId: id, fileId: 'same-physical-file', file, fileName: file.name, dir: '/owned', status: 'created', uploadedSize: 0, error: '' })
  const old = makeItem('old-session')
  const oldDone = queue.addUploadTaskAndWait(old, true)
  const canceled = expect(oldDone).rejects.toThrow('Upload canceled')
  await firstWrite.promise
  queue.removeUpload(old.id)
  snapshot = [{ ...original, token: 'new-session-secret' }]
  await preloadLoginPeers()
  const next = makeItem('new-session')
  const merged = deferred<void>()
  const releaseMerge = deferred<void>()
  io.merge.mockImplementation(async (_upload, args) => {
    expect(new TextDecoder().decode(saved.get(args.fileId))).toBe('new-session-bytes')
    merged.resolve()
    await releaseMerge.promise
    return { value: args.path, size: args.totalSize }
  })
  const done = queue.addUploadTaskAndWait(next, true)
  await new Promise(resolve => setTimeout(resolve, 0))
  expect(getUploadContext(old).key).not.toBe(getUploadContext(next).key)
  expect(uploadChunkOwner(old)).toBe(uploadChunkOwner(next))
  expect(io.invoke.mock.calls.filter(([command]) => command === 'native_upload_chunk_open')).toHaveLength(1)
  releaseAbort.resolve()
  await canceled
  await merged.promise
  expect(io.native.mock.calls.filter(([body]) => body.query === 'delete')).toHaveLength(0)
  expect(new TextDecoder().decode(saved.get('same-physical-file'))).toBe('new-session-bytes')
  releaseMerge.resolve()
  await done
  expect(next.status).toBe('done')
})

it('skips old-session cleanup even without another queue owner and retains cleanup on an unchanged original peer', async () => {
  const { discardUploadChunks } = await import('@/lib/upload/upload')
  const { getUploadContext } = await import('@/lib/upload/context')
  await preloadLoginPeers()
  const makeItem = (id: string): import('@/stores/temp').IUploadItem => ({ id, fileId: id, file: new File(['bytes'], 'sample.bin'), fileName: 'sample.bin', dir: '/owned', status: 'canceled', uploadedSize: 0, error: '' })
  const stale = makeItem('stale')
  getUploadContext(stale)
  snapshot = [{ ...original, token: 'replacement-secret' }]
  await preloadLoginPeers()
  await discardUploadChunks(stale, () => false)
  expect(io.native.mock.calls.filter(([body]) => body.query === 'delete')).toHaveLength(0)
  const originalTarget = makeItem('original-target')
  getUploadContext(originalTarget)
  selection.peer = 'different-peer'
  await discardUploadChunks(originalTarget, () => false)
  expect(io.native.mock.calls.filter(([body]) => body.query === 'delete')).toEqual([
    [{ query: 'delete', variables: { fileId: 'original-target' } }, 30000, { kind: 'peer', peerId: 'peer' }, expect.any(Function)],
  ])
})


it('shares Web chunk ownership across sessions while keeping each original encrypted request token', async () => {
  const { getUploadContext, uploadChunkOwner } = await import('@/lib/upload/context')
  vi.stubGlobal('__IS_TAURI__', false)
  io.token.mockReturnValue('web-old-token')
  const makeItem = (id: string): import('@/stores/temp').IUploadItem => ({ id, fileId: 'web-shared-file', file: new File(['bytes'], 'sample.bin'), fileName: 'sample.bin', dir: '/owned', status: 'created', uploadedSize: 0, error: '' })
  const first = makeItem('web-first')
  const originalContext = getUploadContext(first)
  io.token.mockReturnValue('web-new-token')
  const second = makeItem('web-second')
  const nextContext = getUploadContext(second)
  expect(uploadChunkOwner(first)).toBe(uploadChunkOwner(second))
  expect(originalContext.key).not.toBe(nextContext.key)
  expect(originalContext).toMatchObject({ native: false, token: 'web-old-token', chunkUrl: 'https://unused.test/upload_chunk' })
  expect(nextContext).toMatchObject({ native: false, token: 'web-new-token' })
})
