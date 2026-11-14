import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { IUploadItem } from '@/stores/temp'

const selection = vi.hoisted(() => ({ local: true, peerId: 'peer-a' }))
const io = vi.hoisted(() => ({ scope: vi.fn(), invoke: vi.fn(), gql: vi.fn(), merge: vi.fn(), emit: vi.fn(), progress: vi.fn(), token: vi.fn(), urlToken: vi.fn(), baseUrl: vi.fn(), key: vi.fn(), encrypt: vi.fn() }))
vi.mock('@/lib/api/native-client', () => ({ nativeViewScope: io.scope }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: io.invoke }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => selection.local }))
vi.mock('@/lib/api/time-sync', () => ({ wrapWithReplayProtection: (value: string) => value }))
vi.mock('@/lib/api/api', () => ({ getApiBaseUrl: io.baseUrl, getUrlToken: io.urlToken, proxyUrlFor: (base: string, path: string) => base + path }))
vi.mock('@/lib/api/crypto', () => ({ httpRequestAad: (path: string) => new TextEncoder().encode(`HTTP_REQUEST\0POST\0${path}`), chachaEncrypt: io.encrypt, bitArrayToUint8Array: (bytes: Uint8Array) => bytes }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: io.key }))
vi.mock('@/lib/api/query', () => ({ uploadedChunksGQL: 'query-chunks' }))
vi.mock('@/lib/api/mutation', () => ({ deleteChunksGQL: 'delete-chunks' }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: io.gql, bindGraphqlClient: () => io.gql, getGraphqlContextKey: () => 'local', GqlError: class extends Error {} }))
vi.mock('@/lib/upload/merge-async', () => ({ requestMerge: io.merge }))
vi.mock('@/lib/upload/batch-progress', () => ({
  setUploadStatus: (item: IUploadItem, status: string) => { item.status = status },
  setUploadUploadedSize: (item: IUploadItem, size: number) => { item.uploadedSize = size; io.progress(size) },
}))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: io.token }))
vi.mock('@/lib/device/client-id', () => ({ getDesktopClientId: () => 'test-client', getRemoteClientId: () => selection.peerId }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: io.emit } }))
import { upload } from '@/lib/upload/upload'

const CHUNK = 10 * 1024 * 1024
const WINDOW = 64 * 1024
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(done => { resolve = done })
  return { promise, resolve }
}
function fixtureBytes(size: number) {
  const bytes = new Uint8Array(size)
  for (let i = 0; i < size; i++) bytes[i] = (i * 31 + (i >> 8)) & 255
  return bytes
}

it('preserves synthetic fixture bytes at pattern and window boundaries', () => {
  for (const size of [0, 1, 255, 256, 257, WINDOW - 1, WINDOW, WINDOW + 1]) {
    expect(fixtureBytes(size)).toEqual(Uint8Array.from({ length: size }, (_, i) => (i * 31 + (i >> 8)) & 255))
  }
})

function fixture(size = CHUNK + WINDOW + 17, id = 'file') {
  const bytes = fixtureBytes(size)
  const file = new File([bytes], 'sample.bin')
  const reads: number[] = []
  const fullRead = vi.fn()
  let readWindow = (blob: Blob): Promise<ArrayBuffer> => Blob.prototype.arrayBuffer.call(blob)
  vi.spyOn(file, 'slice').mockImplementation((start, end) => {
    const chunk = Blob.prototype.slice.call(file, start, end)
    vi.spyOn(chunk, 'arrayBuffer').mockImplementation(() => { fullRead(); return Blob.prototype.arrayBuffer.call(chunk) })
    vi.spyOn(chunk, 'slice').mockImplementation((from, to) => {
      const window = Blob.prototype.slice.call(chunk, from, to)
      vi.spyOn(window, 'arrayBuffer').mockImplementation(() => { reads.push(window.size); return readWindow(window) })
      return window
    })
    return chunk
  })
  const item: IUploadItem = { id, fileId: id, file, fileName: file.name, dir: '/owned', status: 'uploading', uploadedSize: 0, error: '' }
  return { item, bytes, reads, fullRead, setRead: (read: typeof readWindow) => { readWindow = read } }
}
function calls(command: string) { return io.invoke.mock.calls.filter(([name]) => name === command) }
function nativeSuccess(command: string) {
  if (command === 'native_upload_chunk_open') return `handle-${calls(command).length}`
  if (command === 'native_upload_chunk_finish') {
    const handle = calls(command).at(-1)![1].handle
    return calls('native_upload_chunk_open')[Number(handle.split('-')[1]) - 1][1].size
  }
}
beforeEach(() => {
  vi.resetAllMocks()
  vi.stubGlobal('__IS_TAURI__', true)
  selection.local = true; selection.peerId = 'peer-a'
  io.scope.mockResolvedValue('document')
  io.baseUrl.mockReturnValue('http://web.test')
  io.token.mockReturnValue('test-token')
  io.urlToken.mockReturnValue('url-token')
  io.key.mockReturnValue(new Uint8Array(32))
  io.encrypt.mockImplementation((_: unknown, text: string) => new TextEncoder().encode(text))
  io.gql.mockResolvedValue({ data: { uploadedChunks: [{ index: 0, sizeBytes: CHUNK }] } })
  io.merge.mockResolvedValue({ value: '/owned/sample.bin' })
  io.invoke.mockImplementation(async command => nativeSuccess(command))
  vi.spyOn(console, 'warn').mockImplementation(() => {})
  vi.spyOn(console, 'error').mockImplementation(() => {})
})
afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); vi.unstubAllGlobals() })

it.each([0, 16, 18])('reuploads a chunk whose reported byte size is %s instead of 17', async sizeBytes => {
  const f = fixture(17)
  io.gql.mockResolvedValue({ data: { uploadedChunks: [{ index: 0, sizeBytes }] } })
  await upload(f.item, false)
  const actual = Buffer.concat(calls('native_upload_chunk_write').map(([, bytes]) => Buffer.from(bytes)))
  expect(actual.equals(Buffer.from(f.bytes))).toBe(true)
})

it.each([0, 1])('reads only bounded windows and reconstructs exact bytes for chunk %s', async index => {
  const f = fixture()
  io.gql.mockResolvedValue({ data: { uploadedChunks: [index === 0 ? { index: 1, sizeBytes: WINDOW + 17 } : { index: 0, sizeBytes: CHUNK }] } })
  let now = 1000
  vi.spyOn(Date, 'now').mockImplementation(() => now += 500)
  await upload(f.item, false)
  const writes = calls('native_upload_chunk_write')
  const actual = Buffer.concat(writes.map(([, bytes]) => Buffer.from(bytes)))
  expect(actual.equals(Buffer.from(f.bytes.subarray(index * CHUNK, Math.min((index + 1) * CHUNK, f.bytes.length))))).toBe(true)
  expect(f.fullRead).not.toHaveBeenCalled()
  expect(f.reads).toEqual(writes.map(([, bytes]) => bytes.length))
  expect(Math.max(...f.reads)).toBe(WINDOW)
  expect(writes.every(([, , options]) => options.headers['x-plain-upload-handle'] === 'handle-1' && options.headers['x-plain-native-scope'] === 'document')).toBe(true)
  expect(io.scope).toHaveBeenCalledOnce()
  expect(calls('native_upload_chunk_open')[0]?.[1].scope).toBe('document')
  const initial = index === 0 ? WINDOW + 17 : CHUNK
  expect(io.progress.mock.calls.map(([size]) => size)).toEqual([initial, ...f.reads.map((_, i) => initial + Math.min((i + 1) * WINDOW, actual.length)), f.bytes.length])
  expect(calls('native_upload_chunk_finish')).toEqual([['native_upload_chunk_finish', { scope: 'document', handle: 'handle-1' }]])
  expect(calls('native_upload_chunk_abort')).toHaveLength(0)
  expect(f.item.status).toBe('done')
})

it.each(['paused', 'canceled'])('does not read ahead of a pending write and aborts %s before finishing', async status => {
  const f = fixture(CHUNK + 17)
  const started = deferred<void>()
  const write = deferred<void>()
  io.invoke.mockImplementation(async command => {
    if (command === 'native_upload_chunk_write') { started.resolve(); await write.promise }
    return nativeSuccess(command)
  })
  const running = upload(f.item, false)
  await started.promise
  expect(f.reads).toEqual([17])
  f.item.status = status
  write.resolve()
  await running
  expect(calls('native_upload_chunk_finish')).toHaveLength(0)
  expect(calls('native_upload_chunk_abort')).toEqual([['native_upload_chunk_abort', { scope: 'document', handle: 'handle-1' }]])
  expect(io.progress.mock.calls).toEqual([[CHUNK]])
  expect(io.merge).not.toHaveBeenCalled()
})

it.each(['open', 'read'])('cancels a pending %s without issuing a later read or write', async phase => {
  const f = fixture()
  const started = deferred<void>()
  const gate = deferred<void>()
  if (phase === 'open') io.invoke.mockImplementation(async command => {
    if (command === 'native_upload_chunk_open') { started.resolve(); await gate.promise }
    return nativeSuccess(command)
  })
  else f.setRead(async blob => { started.resolve(); await gate.promise; return Blob.prototype.arrayBuffer.call(blob) })
  const running = upload(f.item, false)
  await started.promise
  f.item.status = 'paused'
  gate.resolve()
  await running
  expect(f.reads).toHaveLength(phase === 'open' ? 0 : 1)
  expect(f.fullRead).not.toHaveBeenCalled()
  expect(calls('native_upload_chunk_write')).toHaveLength(0)
  expect(calls('native_upload_chunk_finish')).toHaveLength(0)
  expect(calls('native_upload_chunk_abort')).toHaveLength(1)
})

it.each(['open', 'read', 'write', 'finish'])('cleans up %s rejection and retries with a fresh handle', async phase => {
  vi.useFakeTimers()
  const f = fixture(CHUNK + 17)
  let failed = false
  const rejectOnce = () => { if (!failed) { failed = true; throw new RangeError('allocation refused') } }
  f.setRead(async blob => { if (phase === 'read') rejectOnce(); return Blob.prototype.arrayBuffer.call(blob) })
  io.invoke.mockImplementation(async command => {
    if (command === `native_upload_chunk_${phase}`) rejectOnce()
    return nativeSuccess(command)
  })
  const running = upload(f.item, false)
  await vi.runAllTimersAsync()
  await running
  expect(calls('native_upload_chunk_open')).toHaveLength(2)
  expect(calls('native_upload_chunk_abort')).toEqual(phase === 'open' ? [] : [['native_upload_chunk_abort', { scope: 'document', handle: 'handle-1' }]])
  expect(calls('native_upload_chunk_finish').at(-1)).toEqual(['native_upload_chunk_finish', { scope: 'document', handle: 'handle-2' }])
  expect(f.item.status).toBe('done')
  expect(io.merge).toHaveBeenCalledOnce()
})

it('retains the global four-chunk limit across files and releases canceled slots', async () => {
  vi.useFakeTimers()
  const first = fixture(2 * CHUNK + 17, 'first')
  const second = fixture(2 * CHUNK + 17, 'second')
  io.gql.mockResolvedValue({ data: { uploadedChunks: [] } })
  const started = deferred<void>()
  const gate = deferred<void>()
  let pending = 0
  io.invoke.mockImplementation(async command => {
    if (command === 'native_upload_chunk_write') {
      if (++pending === 4) started.resolve()
      await gate.promise
    }
    return nativeSuccess(command)
  })
  const running = Promise.all([upload(first.item, false), upload(second.item, false)])
  await started.promise
  expect(calls('native_upload_chunk_open')).toHaveLength(4)
  expect([...first.reads, ...second.reads]).toEqual([WINDOW, WINDOW, 17, WINDOW])
  first.item.status = second.item.status = 'paused'
  gate.resolve()
  await vi.runAllTimersAsync()
  await running
  expect(calls('native_upload_chunk_abort')).toHaveLength(4)
  expect(calls('native_upload_chunk_finish')).toHaveLength(0)
  io.gql.mockResolvedValue({ data: { uploadedChunks: [{ index: 0, sizeBytes: CHUNK }] } })
  await upload(fixture(CHUNK + 17, 'later').item, false)
  expect(calls('native_upload_chunk_finish')).toHaveLength(1)
})

it('waits for each write before reading the next window', async () => {
  const f = fixture()
  const started = deferred<void>()
  const firstWrite = deferred<void>()
  io.invoke.mockImplementation(async command => {
    if (command === 'native_upload_chunk_write' && calls(command).length === 1) { started.resolve(); await firstWrite.promise }
    return nativeSuccess(command)
  })
  const running = upload(f.item, false)
  await started.promise
  expect(f.reads).toEqual([WINDOW])
  expect(calls('native_upload_chunk_write')).toHaveLength(1)
  firstWrite.resolve()
  await running
  expect(f.reads).toEqual([WINDOW, 17])
  expect(f.fullRead).not.toHaveBeenCalled()
})

it('retains the five-attempt retry limit and aborts every rejected buffer allocation', async () => {
  vi.useFakeTimers()
  const f = fixture(CHUNK + 17)
  f.setRead(async () => { throw new RangeError('allocation refused') })
  const running = upload(f.item, false)
  await vi.runAllTimersAsync()
  await running
  expect(calls('native_upload_chunk_open')).toHaveLength(5)
  expect(calls('native_upload_chunk_abort').map(([, args]) => args.handle)).toEqual(['handle-1', 'handle-2', 'handle-3', 'handle-4', 'handle-5'])
  expect(calls('native_upload_chunk_write')).toHaveLength(0)
  expect(calls('native_upload_chunk_finish')).toHaveLength(0)
  expect(io.merge).not.toHaveBeenCalled()
  expect(f.item.status).toBe('error')
  expect(f.item.error).toBe('Failed to upload: chunk 1')
  expect(io.scope).toHaveBeenCalledOnce()
})

it('stops before finish when the final progress callback cancels the upload', async () => {
  const f = fixture(CHUNK + 17)
  let now = 1000
  vi.spyOn(Date, 'now').mockImplementation(() => now += 500)
  io.progress.mockImplementation(size => { if (size === CHUNK + 17) f.item.status = 'canceled' })
  await upload(f.item, false)
  expect(calls('native_upload_chunk_write')).toHaveLength(1)
  expect(calls('native_upload_chunk_finish')).toHaveLength(0)
  expect(calls('native_upload_chunk_abort')).toHaveLength(1)
  expect(io.merge).not.toHaveBeenCalled()
})

it('preserves monotonic progress and exact bytes after a partial write retry', async () => {
  vi.useFakeTimers()
  const f = fixture(CHUNK + 2 * WINDOW + 17)
  f.setRead(async blob => {
    vi.setSystemTime(Date.now() + 500)
    return Blob.prototype.arrayBuffer.call(blob)
  })
  io.invoke.mockImplementation(async command => {
    if (command === 'native_upload_chunk_write' && calls(command).length === 2) throw new Error('write refused')
    return nativeSuccess(command)
  })
  const running = upload(f.item, false)
  await vi.runAllTimersAsync()
  await running
  expect(io.progress.mock.calls.map(([size]) => size)).toEqual([CHUNK, CHUNK + WINDOW, CHUNK + 2 * WINDOW, f.bytes.length, f.bytes.length])
  const retried = calls('native_upload_chunk_write').filter(([, , options]) => options.headers['x-plain-upload-handle'] === 'handle-2')
  expect(Buffer.concat(retried.map(([, bytes]) => Buffer.from(bytes))).equals(Buffer.from(f.bytes.subarray(CHUNK)))).toBe(true)
  expect(calls('native_upload_chunk_abort')).toHaveLength(1)
  expect(calls('native_upload_chunk_finish')).toHaveLength(1)
  expect(f.item.status).toBe('done')
})


it.each([true, false])('routes all Desktop sizes through native chunks (local=%s), without Web credentials or multipart', async local => {
  selection.local = local
  io.gql.mockResolvedValue({ data: { uploadedChunks: [] } })
  const xhr = vi.fn(function () { throw new Error('Desktop must not use XHR') })
  const form = vi.fn(function () { throw new Error('Desktop must not use multipart') })
  vi.stubGlobal('XMLHttpRequest', xhr)
  vi.stubGlobal('FormData', form)
  for (const size of [0, 17, CHUNK, CHUNK + 17]) {
    io.invoke.mockClear(); io.merge.mockClear()
    const f = fixture(size, `size-${size}`)
    await upload(f.item, true)
    expect(f.item.status).toBe('done')
    const opens = calls('native_upload_chunk_open')
    expect(opens.map(([, args]) => args)).toEqual(Array.from({ length: Math.max(1, Math.ceil(size / CHUNK)) }, (_, index) => ({
      scope: 'document', target: local ? { kind: 'local' } : { kind: 'peer', peerId: 'peer-a' }, fileId: f.item.fileId, index, size: Math.min(CHUNK, size - index * CHUNK),
    })))
    expect(calls('native_upload_chunk_finish')).toHaveLength(opens.length)
    expect(io.merge).toHaveBeenCalledWith(f.item, { fileId: f.item.fileId, totalChunks: opens.length, path: '/owned/sample.bin', replace: true, totalSize: size })
    if (size === 0) expect(calls('native_upload_chunk_write')).toHaveLength(0)
    else expect(Buffer.concat(calls('native_upload_chunk_write').map(([, bytes]) => Buffer.from(bytes))).byteLength).toBe(size)
  }
  expect(xhr).not.toHaveBeenCalled()
  expect(form).not.toHaveBeenCalled()
  for (const fn of [io.token, io.urlToken, io.baseUrl, io.key, io.encrypt]) expect(fn).not.toHaveBeenCalled()
})

it('retries an incorrect finish receipt with its captured peer target and scope', async () => {
  vi.useFakeTimers()
  selection.local = false
  const f = fixture(CHUNK + 17)
  io.invoke.mockImplementation(async command => {
    if (command === 'native_upload_chunk_finish' && calls(command).length === 1) {
      selection.peerId = 'peer-b'
      return 0
    }
    return nativeSuccess(command)
  })
  const running = upload(f.item, false)
  await vi.runAllTimersAsync()
  await running
  expect(calls('native_upload_chunk_open').map(([, args]) => [args.target, args.scope])).toEqual([
    [{ kind: 'peer', peerId: 'peer-a' }, 'document'], [{ kind: 'peer', peerId: 'peer-a' }, 'document'],
  ])
  expect(calls('native_upload_chunk_abort')).toHaveLength(1)
  expect(io.scope).toHaveBeenCalledOnce()
  expect(f.item.status).toBe('done')
})

it.each([0, 17, CHUNK + 17])('preserves Web XHR and multipart for %s bytes', async size => {
  vi.stubGlobal('__IS_TAURI__', false)
  selection.local = false
  io.gql.mockResolvedValue({ data: { uploadedChunks: [] } })
  const opened: string[] = []
  const headers: unknown[][] = []
  const bodies: FormData[] = []
  class Xhr {
    upload = new EventTarget()
    readyState = 0
    status = 201
    responseText = '/owned/sample.bin'
    onreadystatechange = () => {}
    open(_method: string, url: string) { opened.push(url) }
    setRequestHeader(...args: unknown[]) { headers.push(args) }
    send(body: FormData) {
      bodies.push(body)
      queueMicrotask(async () => {
        if (size > CHUNK) {
          const info = JSON.parse(await (body.get('info') as Blob).text())
          this.responseText = `${info.index}:${(body.get('file') as Blob).size}`
        }
        this.readyState = 4
        this.onreadystatechange()
      })
    }
  }
  vi.stubGlobal('XMLHttpRequest', Xhr)
  const f = fixture(size)
  await upload(f.item, false)
  expect(f.item.status).toBe('done')
  expect(opened).toEqual(Array(size > CHUNK ? 2 : 1).fill(`http://web.test/${size > CHUNK ? 'upload_chunk' : 'upload'}`))
  expect(bodies.every(body => body instanceof FormData)).toBe(true)
  expect(io.encrypt.mock.calls.map(([, , aad]) => new TextDecoder().decode(aad))).toEqual(
    opened.map(() => `HTTP_REQUEST\0POST\0/${size > CHUNK ? 'upload_chunk' : 'upload'}`)
  )
  expect(headers).toEqual(opened.map(() => ['c-id', 'test-client']))
  expect(io.token).toHaveBeenCalledOnce()
  expect(io.invoke).not.toHaveBeenCalled()
})
