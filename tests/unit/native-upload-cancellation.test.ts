import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { IUploadItem } from '@/stores/temp'

const io = vi.hoisted(() => ({ invoke: vi.fn(), scope: vi.fn(), gql: vi.fn(), merge: vi.fn(), emit: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: io.invoke }))
vi.mock('@/lib/api/native-client', () => ({ nativeViewScope: io.scope }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => 'original-peer' }))
vi.mock('@/lib/api/api', () => ({}))
vi.mock('@/lib/device/current', () => ({}))
vi.mock('@/lib/api/time-sync', () => ({}))
vi.mock('@/lib/api/crypto', () => ({}))
vi.mock('@/lib/api/file', () => ({}))
vi.mock('@/lib/api/query', () => ({ uploadedChunksGQL: 'chunks' }))
vi.mock('@/lib/api/mutation', () => ({ deleteChunksGQL: 'delete' }))
vi.mock('@/lib/api/gql-client', () => ({ getGraphqlContextKey: () => 'original-peer', bindGraphqlClient: () => io.gql, GqlError: class extends Error {} }))
vi.mock('@/lib/upload/merge-async', () => ({ requestMerge: io.merge }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: io.emit } }))

const CHUNK = 10 * 1024 * 1024
let queue: typeof import('@/lib/upload/upload-queue')
let handles: Map<string, { size: number; fileId: string }>
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail })
  return { promise, resolve, reject }
}
function item(id: string, size = 17): IUploadItem {
  return { id, batchId: id, fileId: id, file: new File([new Uint8Array(size)], 'sample.bin'), fileName: 'sample.bin', dir: '/owned', status: 'created', uploadedSize: 0, error: '' }
}
function calls(command: string) { return io.invoke.mock.calls.filter(([name]) => name === command) }
function nativeSuccess(command: string, args: any) {
  if (command === 'native_upload_chunk_open') {
    const handle = `handle-${handles.size}`
    handles.set(handle, args)
    return handle
  }
  if (command === 'native_upload_chunk_finish') return handles.get(args.handle)?.size
}
beforeEach(async () => {
  vi.resetModules(); vi.resetAllMocks()
  handles = new Map()
  vi.stubGlobal('__IS_TAURI__', true)
  vi.stubGlobal('window', new EventTarget())
  io.scope.mockResolvedValue('original-document')
  io.gql.mockResolvedValue({ data: { uploadedChunks: [], deleteChunks: true } })
  io.invoke.mockImplementation(async (command, args) => nativeSuccess(command, args))
  io.merge.mockImplementation(async (_item, args) => ({ value: args.path, size: args.totalSize }))
  queue = await import('@/lib/upload/upload-queue')
  vi.spyOn(console, 'warn').mockImplementation(() => {})
})
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals() })

it.each(['write', 'finish'])('queue pause interrupts pending native %s and releases the global slot', async phase => {
  const started = deferred<void>()
  const blocked = deferred<void>()
  io.invoke.mockImplementation(async (command, args, options) => {
    const handle = command === 'native_upload_chunk_write' ? options.headers['x-plain-upload-handle'] : args.handle
    if (command === `native_upload_chunk_${phase}` && handle === 'handle-0') { started.resolve(); await blocked.promise }
    if (command === 'native_upload_chunk_abort' && handle === 'handle-0') blocked.reject(new Error('canceled by abort'))
    return nativeSuccess(command, args)
  })
  const current = item('paused')
  queue.addUploadTask(current, false)
  await started.promise
  queue.pauseUploadsByBatch(current.id)
  expect(calls('native_upload_chunk_abort')).toEqual([['native_upload_chunk_abort', { scope: 'original-document', handle: 'handle-0' }]])
  queue.removeUpload(current.id)
  await vi.waitFor(() => expect(io.gql).toHaveBeenCalledWith('delete', { fileId: 'paused' }))
  expect(io.merge).not.toHaveBeenCalled()
  expect(calls('native_upload_chunk_open')).toHaveLength(1)
  const later = ['a', 'b', 'c', 'd'].map(id => item(id))
  await Promise.all(later.map(upload => queue.addUploadTaskAndWait(upload, false)))
  expect(later.every(upload => upload.status === 'done')).toBe(true)
  expect(calls('native_upload_chunk_abort')).toHaveLength(1)
  expect(io.merge).toHaveBeenCalledTimes(4)
})

it.each(['write', 'finish'])('queue remove interrupts pending native %s without waiting for the transfer', async phase => {
  const started = deferred<void>()
  const blocked = deferred<void>()
  io.invoke.mockImplementation(async (command, args) => {
    if (command === `native_upload_chunk_${phase}`) { started.resolve(); await blocked.promise }
    if (command === 'native_upload_chunk_abort') blocked.reject(new Error('canceled by abort'))
    return nativeSuccess(command, args)
  })
  const current = item('removed')
  const completion = queue.addUploadTaskAndWait(current, false)
  const canceled = expect(completion).rejects.toThrow('Upload canceled')
  await started.promise
  queue.removeUpload(current.id)
  await canceled
  await vi.waitFor(() => expect(io.gql).toHaveBeenCalledWith('delete', { fileId: 'removed' }))
  expect(calls('native_upload_chunk_abort')).toEqual([['native_upload_chunk_abort', { scope: 'original-document', handle: 'handle-0' }]])
  expect(io.merge).not.toHaveBeenCalled()
  expect(current.status).toBe('canceled')
  expect(queue.getUploadQueueStatus().running).toBe(0)
})

it.each(['scope', 'open'])('latches cancellation before pending %s returns, even after immediate resume', async phase => {
  const started = deferred<void>()
  const release = deferred<void>()
  const resumed = deferred<void>()
  const finish = deferred<void>()
  if (phase === 'scope') io.scope.mockImplementationOnce(async () => { started.resolve(); await release.promise; return 'original-document' })
  io.invoke.mockImplementation(async (command, args) => {
    if (phase === 'open' && command === 'native_upload_chunk_open' && calls(command).length === 1) { started.resolve(); await release.promise }
    if (command === 'native_upload_chunk_finish') { resumed.resolve(); await finish.promise }
    return nativeSuccess(command, args)
  })
  const current = item('resumed')
  const completion = queue.addUploadTaskAndWait(current, false)
  await started.promise
  queue.pauseUploadsByBatch(current.id)
  queue.resumeUploadsByBatch(current.id)
  release.resolve()
  await resumed.promise
  expect(calls('native_upload_chunk_open')).toHaveLength(phase === 'open' ? 2 : 1)
  expect(calls('native_upload_chunk_abort')).toEqual(phase === 'open' ? [['native_upload_chunk_abort', { scope: 'original-document', handle: 'handle-0' }]] : [])
  expect(calls('native_upload_chunk_write').map(([, , options]) => options.headers['x-plain-upload-handle'])).toEqual([phase === 'open' ? 'handle-1' : 'handle-0'])
  expect(calls('native_upload_chunk_open').every(([, args]) => args.target.peerId === 'original-peer')).toBe(true)
  expect(io.merge).not.toHaveBeenCalled()
  expect(queue.getUploadQueueStatus().running).toBe(1)
  finish.resolve()
  await completion
  expect(current.status).toBe('done')
  expect(io.merge).toHaveBeenCalledOnce()
  expect(queue.getUploadQueueStatus().total).toBe(0)
})

it('aborts every parallel handle once, releases waiters and leaves another upload unaffected', async () => {
  const started = deferred<void>()
  const blocked = new Map<string, ReturnType<typeof deferred<void>>>()
  io.invoke.mockImplementation(async (command, args, options) => {
    if (command === 'native_upload_chunk_write') {
      const handle = options.headers['x-plain-upload-handle']
      if (handles.get(handle)?.fileId === 'parallel') {
        const waiting = deferred<void>()
        blocked.set(handle, waiting)
        if (blocked.size === 3) started.resolve()
        await waiting.promise
      }
    }
    if (command === 'native_upload_chunk_abort') blocked.get(args.handle)?.reject(new Error('canceled by abort'))
    return nativeSuccess(command, args)
  })
  const current = item('parallel', CHUNK * 2 + 17)
  queue.addUploadTask(current, false)
  await started.promise
  const independent = item('independent')
  const done = queue.addUploadTaskAndWait(independent, false)
  queue.removeUploadsByBatch(current.id)
  expect(calls('native_upload_chunk_abort')).toHaveLength(3)
  await done
  await vi.waitFor(() => expect(io.gql).toHaveBeenCalledWith('delete', { fileId: 'parallel' }))
  expect(new Set(calls('native_upload_chunk_abort').map(([, args]) => args.handle)).size).toBe(3)
  expect(io.merge.mock.calls.map(([upload]) => upload.id)).toEqual(['independent'])
  expect(independent.status).toBe('done')
})
