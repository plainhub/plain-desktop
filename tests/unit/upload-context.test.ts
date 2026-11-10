import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { IUploadItem } from '@/stores/temp'

const target = vi.hoisted(() => ({ key: 'a', native: true }))
const io = vi.hoisted(() => ({ gql: vi.fn(), invoke: vi.fn(), scope: vi.fn(), subscribe: vi.fn(), stop: vi.fn(), emit: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: io.invoke }))
vi.mock('@/lib/api/native-client', () => ({ nativeViewScope: io.scope }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => target.native }))
vi.mock('@/lib/api/time-sync', () => ({ wrapWithReplayProtection: (value: string) => value }))
vi.mock('@/lib/api/api', () => ({ getApiBaseUrl: () => `http://${target.key}.test`, getUrlToken: () => target.key, proxyUrlFor: (base: string, path: string) => base + path }))
vi.mock('@/lib/api/crypto', () => ({ httpRequestAad: (path: string) => new TextEncoder().encode(`http-request\0POST\0${path}`), chachaEncrypt: (_: unknown, text: string) => new TextEncoder().encode(text), bitArrayToUint8Array: (bytes: Uint8Array) => bytes }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: () => new Uint8Array(32) }))
vi.mock('@/lib/api/query', () => ({ uploadedChunksGQL: 'query-chunks', mergeStatusGQL: 'merge-status' }))
vi.mock('@/lib/api/mutation', () => ({ deleteChunksGQL: 'delete-chunks', mergeChunksGQL: 'merge-chunks', mergeAppFileChunksGQL: 'merge-app-chunks' }))
vi.mock('@/lib/api/gql-client', () => ({
  getGraphqlContextKey: () => target.key,
  bindGraphqlClient: () => { const key = target.key; return (...args: unknown[]) => io.gql(key, ...args) },
  GqlError: class extends Error {},
}))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { subscribe: io.subscribe } }))
vi.mock('@/lib/upload/batch-progress', () => ({
  setUploadStatus: (item: IUploadItem, status: string) => { item.status = status },
  setUploadUploadedSize: (item: IUploadItem, size: number) => { item.uploadedSize = size },
  resetUploadProgress: vi.fn(),
}))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => target.key }))
vi.mock('@/lib/device/client-id', () => ({ getDesktopClientId: () => `client-${target.key}`, getRemoteClientId: () => target.key }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: io.emit } }))
import { upload, discardUploadChunks } from '@/lib/upload/upload'
import { getUploadContext } from '@/lib/upload/context'
import { requestMerge } from '@/lib/upload/merge-async'

const CHUNK = 10 * 1024 * 1024
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(done => { resolve = done })
  return { promise, resolve }
}
function item(id = 'upload', size = CHUNK + 17): IUploadItem {
  return { id, batchId: id, fileId: 'same-file', file: new File([new Uint8Array(size)], 'sample.bin'), fileName: 'sample.bin', dir: '/owned', status: 'uploading', uploadedSize: 0, error: '' }
}
const mergeArgs = { fileId: 'same-file', totalChunks: 2, path: '/owned/sample.bin', replace: false, totalSize: CHUNK + 17 }
beforeEach(() => {
  vi.resetAllMocks()
  vi.stubGlobal('__IS_TAURI__', true)
  target.key = 'a'; target.native = true
  io.scope.mockResolvedValue('original-document')
  io.invoke.mockImplementation(async command => command === 'native_upload_chunk_open' ? 'original-handle' : command === 'native_upload_chunk_finish' ? 17 : undefined)
  io.gql.mockImplementation(async (_key, doc) => ({ data: doc === 'query-chunks' ? { uploadedChunks: [`0:${CHUNK}`] } : doc === 'delete-chunks' ? { deleteChunks: true } : { mergeChunks: { status: 'DONE', value: '/owned/sample.bin', mergedSize: CHUNK + 17 } } }))
  io.subscribe.mockReturnValue(io.stop)
  vi.spyOn(console, 'warn').mockImplementation(() => {})
})
afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); vi.unstubAllGlobals() })

it('keeps canceled cleanup on its original target after a pending chunk query', async () => {
  const query = deferred<unknown>()
  const started = deferred<void>()
  io.gql.mockImplementationOnce(() => { started.resolve(); return query.promise })
  const current = item()
  const running = upload(current, false)
  await started.promise
  target.key = 'b'; target.native = false
  const cleanup = discardUploadChunks(current, () => false)
  expect(io.gql).toHaveBeenCalledOnce()
  query.resolve({ data: { uploadedChunks: [] } })
  await Promise.all([running, cleanup])
  expect(current.error).toBe('request_context_changed')
  expect(io.gql.mock.calls).toEqual([['a', 'query-chunks', { fileId: 'same-file' }, 120000], ['a', 'delete-chunks', { fileId: 'same-file' }]])
  expect(io.invoke).not.toHaveBeenCalled()
})

it.each(['scope', 'open', 'write', 'finish'])('rejects a changed target after pending native %s and reuses its original scope for cleanup', async phase => {
  target.native = false
  const gate = deferred<void>()
  const started = deferred<void>()
  if (phase === 'scope') io.scope.mockImplementationOnce(async () => { started.resolve(); await gate.promise; return 'original-document' })
  else io.invoke.mockImplementation(async command => {
    if (command === `native_upload_chunk_${phase}`) { started.resolve(); await gate.promise }
    return command === 'native_upload_chunk_open' ? 'original-handle' : command === 'native_upload_chunk_finish' ? 17 : undefined
  })
  const current = item()
  const running = upload(current, false)
  await started.promise
  target.key = 'b'; target.native = false
  gate.resolve()
  await running
  expect(current.status).toBe('error')
  expect(current.error).toBe('request_context_changed')
  expect(io.gql).toHaveBeenCalledOnce()
  expect(io.scope).toHaveBeenCalledOnce()
  const calls = io.invoke.mock.calls
  for (const [, args] of calls.filter(([command]) => command === 'native_upload_chunk_open')) expect(args.target).toEqual({ kind: 'peer', peerId: 'a' })
  expect(calls.filter(([command]) => command === 'native_upload_chunk_finish')).toHaveLength(phase === 'finish' ? 1 : 0)
  expect(calls.filter(([command]) => command === 'native_upload_chunk_abort')).toEqual(phase === 'open' || phase === 'write' ? [['native_upload_chunk_abort', { scope: 'original-document', handle: 'original-handle' }]] : [])
  if (phase === 'scope') expect(io.invoke).not.toHaveBeenCalled()
})

it('does not rebind a paused or retried upload to the newly selected device', async () => {
  const current = item()
  const context = getUploadContext(current)
  current.status = 'paused'
  target.key = 'b'
  current.status = 'uploading'
  await expect(upload(current, false)).rejects.toThrow('request_context_changed')
  expect(getUploadContext(current)).toBe(context)
  expect(io.gql).not.toHaveBeenCalled()
  expect(io.invoke).not.toHaveBeenCalled()
})

it('binds queued uploads before they get a file slot and keeps per-target cleanup ownership separate', async () => {
  vi.stubGlobal('window', new EventTarget())
  vi.resetModules()
  const queue = await import('@/lib/upload/upload-queue')
  const contexts = await import('@/lib/upload/context')
  const gate = deferred<unknown>()
  io.gql.mockReturnValue(gate.promise)
  const uploads = ['first', 'second', 'third', 'queued'].map(id => item(id))
  uploads.forEach(current => queue.addUploadTask(current, false))
  expect(contexts.getUploadContext(uploads[3]).key).toBe('a')
  queue.pauseUploadsByBatch('queued')
  target.key = 'b'
  queue.resumeUploadsByBatch('queued')
  expect(contexts.getUploadContext(uploads[3]).key).toBe('a')
  const remote = item('remote')
  queue.addUploadTask(remote, false)
  queue.pauseUploadsByBatch('remote')
  queue.removeUpload('queued')
  queue.removeUpload('remote')
  expect(io.gql.mock.calls.every(([key]) => key === 'a' || key === 'b')).toBe(true)
  for (const current of uploads.slice(0, 3)) queue.removeUpload(current.id)
  gate.resolve({ data: { uploadedChunks: [] } })
  await vi.waitFor(() => expect(io.gql.mock.calls.filter(([, doc]) => doc === 'delete-chunks').map(([key]) => key).sort()).toEqual(['a', 'a', 'a', 'a', 'b']))
})

it.each(['event', 'poll', 'poll-pending'])('rejects a merge result from a changed target during %s and releases watchers', async phase => {
  vi.useFakeTimers()
  io.gql.mockResolvedValue({ data: { mergeChunks: { status: 'STARTED' } } })
  const subscribed = deferred<void>()
  io.subscribe.mockImplementation(() => { subscribed.resolve(); return io.stop })
  const current = item()
  const result = requestMerge(current, mergeArgs)
  await subscribed.promise
  const poll = deferred<unknown>()
  if (phase === 'poll-pending') {
    io.gql.mockReturnValueOnce(poll.promise)
    await vi.advanceTimersByTimeAsync(15000)
  }
  target.key = 'b'
  if (phase === 'event') io.subscribe.mock.calls[0][1]({ fileId: 'same-file', ok: true, value: '/wrong-device', mergedSize: CHUNK + 17 })
  else await vi.advanceTimersByTimeAsync(1000)
  expect(await result).toEqual({ error: 'request_context_changed' })
  poll.resolve({ data: { mergeStatus: { status: 'DONE', value: '/wrong-device' } } })
  await vi.runAllTimersAsync()
  expect(io.stop).toHaveBeenCalledOnce()
  expect(vi.getTimerCount()).toBe(0)
  expect(io.gql.mock.calls.every(([key]) => key === 'a')).toBe(true)
  expect(io.gql).toHaveBeenCalledTimes(phase === 'poll-pending' ? 2 : 1)
})

it('preserves the original upload URL and client header and rejects stale direct completion', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  target.native = false
  const opened: unknown[][] = []
  const headers: unknown[][] = []
  let xhr!: FakeXHR
  class FakeXHR {
    upload = new EventTarget()
    readyState = 0
    status = 201
    responseText = '/owned/sample.bin'
    onreadystatechange = () => {}
    open(...args: unknown[]) { opened.push(args) }
    setRequestHeader(...args: unknown[]) { headers.push(args) }
    send() { xhr = this }
    abort() {}
  }
  vi.stubGlobal('XMLHttpRequest', FakeXHR)
  const current = item('small', 17)
  const running = upload(current, false)
  expect(opened).toEqual([['POST', 'http://a.test/upload', true]])
  expect(headers).toEqual([['c-id', 'client-a']])
  target.key = 'b'
  xhr.readyState = 4
  xhr.onreadystatechange()
  expect(await running).toEqual({ error: 'request_context_changed' })
  expect(current.status).toBe('error')
  expect(opened).toHaveLength(1)
})

it('waits for every original-target handle to abort before deleting its chunks', async () => {
  const started = deferred<void>()
  const first = deferred<void>()
  const second = deferred<void>()
  const firstAborted = deferred<void>()
  let writes = 0
  io.gql.mockImplementation(async (_key, doc) => ({ data: doc === 'query-chunks' ? { uploadedChunks: [] } : { deleteChunks: true } }))
  io.invoke.mockImplementation(async (command, args) => {
    if (command === 'native_upload_chunk_open') return `handle-${args.index}`
    if (command === 'native_upload_chunk_write') {
      const gate = ++writes === 1 ? first : second
      if (writes === 2) started.resolve()
      await gate.promise
    }
    if (command === 'native_upload_chunk_abort') firstAborted.resolve()
  })
  const current = item()
  const running = upload(current, false)
  await started.promise
  target.key = 'b'
  const cleanup = discardUploadChunks(current, () => false)
  first.resolve()
  await firstAborted.promise
  expect(io.gql).toHaveBeenCalledOnce()
  second.resolve()
  await Promise.all([running, cleanup])
  expect(io.invoke.mock.calls.filter(([command]) => command === 'native_upload_chunk_abort')).toHaveLength(2)
  expect(io.gql.mock.calls.at(-1)).toEqual(['a', 'delete-chunks', { fileId: 'same-file' }])
  expect(io.invoke.mock.calls.filter(([command]) => command === 'native_upload_chunk_finish')).toHaveLength(0)
})

it('stops chunk retries when the selected target changes during backoff', async () => {
  vi.useFakeTimers()
  const current = item()
  io.invoke.mockRejectedValue(new Error('native unavailable'))
  const running = upload(current, false)
  await vi.advanceTimersByTimeAsync(100)
  target.key = 'b'; target.native = false
  await vi.runAllTimersAsync()
  await running
  expect(current.error).toBe('request_context_changed')
  expect(io.invoke).toHaveBeenCalledOnce()
  expect(io.gql).toHaveBeenCalledOnce()
})

it('rejects stale pending merge mutation results without subscribing to the new target', async () => {
  const pending = deferred<unknown>()
  io.gql.mockReturnValueOnce(pending.promise)
  const result = requestMerge(item(), mergeArgs)
  target.key = 'b'
  pending.resolve({ data: { mergeChunks: { status: 'STARTED' } } })
  expect(await result).toEqual({ error: 'request_context_changed' })
  expect(io.subscribe).not.toHaveBeenCalled()
})
