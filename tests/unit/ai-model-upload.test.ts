import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { IUploadItem } from '@/stores/temp'

const target = vi.hoisted(() => ({ key: 'a' }))
const io = vi.hoisted(() => ({ invoke: vi.fn(), scope: vi.fn(), gql: vi.fn(), merge: vi.fn(), toast: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string, args?: unknown) => `${key}:${JSON.stringify(args)}` }) }))
vi.mock('@/components/toaster', () => ({ default: io.toast }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: io.invoke }))
vi.mock('@/lib/api/native-client', () => ({ nativeViewScope: io.scope }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => target.key }))
vi.mock('@/lib/api/api', () => ({}))
vi.mock('@/lib/device/current', () => ({}))
vi.mock('@/lib/api/time-sync', () => ({}))
vi.mock('@/lib/api/crypto', () => ({}))
vi.mock('@/lib/api/file', () => ({}))
vi.mock('@/lib/api/query', () => ({ uploadedChunksGQL: 'chunks' }))
vi.mock('@/lib/api/mutation', () => ({ deleteChunksGQL: 'delete' }))
vi.mock('@/lib/api/gql-client', () => ({
  getGraphqlContextKey: () => target.key,
  bindGraphqlClient: () => { const key = target.key; return (...args: unknown[]) => io.gql(key, ...args) },
  GqlError: class extends Error {},
}))
vi.mock('@/lib/upload/merge-async', () => ({ requestMerge: io.merge }))

let hook: ReturnType<typeof import('@/hooks/ai-model-upload')['useAIModelUpload']>
let emitter: typeof import('@/plugins/eventbus')['default']
let handles: Map<string, number>
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(done => { resolve = done })
  return { promise, resolve }
}
function bundle() {
  const image = new File(['image-model'], 'image.bin', { lastModified: 100 })
  const text = new File(['text-and-tokenizer'], 'text.bin', { lastModified: 100 })
  const manifest = new File([JSON.stringify({ formatVersion: 1, files: [image, text].map(file => ({ name: file.name, size: file.size, sha256: 'ab'.repeat(32) })), image: { file: image.name }, text: { file: text.name }, tokenizer: { file: text.name } })], 'manifest.json', { lastModified: 100 })
  return { files: [manifest, text, image] as unknown as FileList, manifest, total: image.size + text.size + manifest.size }
}
function nativeSuccess(command: string, args: any) {
  if (command === 'native_upload_chunk_open') {
    const handle = `handle-${handles.size}`
    handles.set(handle, args.size)
    return handle
  }
  if (command === 'native_upload_chunk_finish') return handles.get(args.handle)
}
beforeEach(async () => {
  vi.resetModules(); vi.resetAllMocks()
  target.key = 'a'; handles = new Map()
  vi.stubGlobal('__IS_TAURI__', true)
  vi.stubGlobal('window', new EventTarget())
  vi.stubGlobal('XMLHttpRequest', vi.fn(function () { throw new Error('Desktop model uploads must use native IPC') }))
  vi.stubGlobal('FormData', vi.fn(function () { throw new Error('Desktop model uploads must not use multipart') }))
  io.scope.mockResolvedValue('document')
  io.gql.mockResolvedValue({ data: { uploadedChunks: [], deleteChunks: true } })
  io.invoke.mockImplementation(async (command, args) => nativeSuccess(command, args))
  io.merge.mockImplementation(async (_item, args) => ({ value: args.path, size: args.totalSize }))
  emitter = (await import('@/plugins/eventbus')).default
  hook = (await import('@/hooks/ai-model-upload')).useAIModelUpload()
  vi.spyOn(console, 'warn').mockImplementation(() => {})
})
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals() })

it('uses the real shared queue and native upload for sequential assets, then the replacement manifest', async () => {
  const data = bundle()
  const merged = deferred<void>()
  const release = deferred<void>()
  io.merge.mockImplementationOnce(async (_item, args) => { merged.resolve(); await release.promise; return { value: args.path, size: args.totalSize } })
  const running = hook.startUpload(data.files, '/models')
  await merged.promise
  expect(io.invoke.mock.calls.filter(([command]) => command === 'native_upload_chunk_open')).toHaveLength(1)
  expect(hook.uploadProgress.value).toBe(Math.round(('image-model'.length / data.total) * 100))
  expect(hook.uploading.value).toBe(true)
  release.resolve()
  await running
  expect(io.merge.mock.calls.map(([, args]) => [args.path, args.replace])).toEqual([
    ['/models/image.bin', true], ['/models/text.bin', true], ['/models/manifest.json', true],
  ])
  expect(io.invoke.mock.calls.filter(([command]) => command === 'native_upload_chunk_open').map(([, args]) => args.target)).toEqual(Array(3).fill({ kind: 'peer', peerId: 'a' }))
  expect(hook.uploadDone.value).toBe(true)
  expect(hook.uploadProgress.value).toBe(100)
  expect(hook.uploading.value).toBe(false)
  expect(io.toast).not.toHaveBeenCalled()
})

it('cancels an in-flight model file, cleans its handle and never writes the manifest', async () => {
  const started = deferred<void>()
  const release = deferred<void>()
  io.invoke.mockImplementation(async (command, args) => {
    if (command === 'native_upload_chunk_write') { started.resolve(); await release.promise }
    return nativeSuccess(command, args)
  })
  const running = hook.startUpload(bundle().files, '/models')
  await started.promise
  hook.cancelUpload()
  release.resolve()
  await running
  await vi.waitFor(() => expect(io.invoke.mock.calls.filter(([command]) => command === 'native_upload_chunk_abort')).toHaveLength(1))
  expect(io.merge).not.toHaveBeenCalled()
  expect(hook.uploadProgress.value).toBe(0)
  expect(hook.uploadDone.value).toBe(false)
  expect(hook.uploading.value).toBe(false)
  expect(io.toast).not.toHaveBeenCalled()
  expect(io.gql.mock.calls.every(([key]) => key === 'a')).toBe(true)
})

it('keeps all assets bound to the starting target when the device changes between files', async () => {
  io.merge.mockImplementationOnce(async (_item, args) => {
    queueMicrotask(() => { target.key = 'b' })
    return { value: args.path, size: args.totalSize }
  })
  await hook.startUpload(bundle().files, '/models')
  expect(io.invoke.mock.calls.filter(([command]) => command === 'native_upload_chunk_open')).toHaveLength(1)
  expect(io.merge).toHaveBeenCalledOnce()
  expect(hook.uploadDone.value).toBe(false)
  expect(io.toast).toHaveBeenCalledWith(expect.stringContaining('request_context_changed'), 'error')
  expect(io.gql.mock.calls.every(([key]) => key === 'a')).toBe(true)
})

it('captures the target before asynchronous manifest validation', async () => {
  const data = bundle()
  const validation = deferred<string>()
  const original = await data.manifest.text()
  vi.spyOn(data.manifest, 'text').mockReturnValue(validation.promise)
  const running = hook.startUpload(data.files, '/models')
  target.key = 'b'
  validation.resolve(original)
  await running
  expect(io.invoke).not.toHaveBeenCalled()
  expect(io.gql).not.toHaveBeenCalled()
  expect(io.toast).toHaveBeenCalledWith(expect.stringContaining('request_context_changed'), 'error')
})

it('rejects invalid declarations without uploading', async () => {
  await hook.startUpload([new File(['{}'], 'manifest.json')] as unknown as FileList, '/models')
  expect(io.invoke).not.toHaveBeenCalled()
  expect(io.toast).toHaveBeenCalledWith(expect.stringContaining('ai.wrong_files'), 'error')
  expect(hook.uploading.value).toBe(false)
})

it('ignores unrelated upload progress and stops after a failed model save', async () => {
  const started = deferred<void>()
  const release = deferred<void>()
  io.merge.mockImplementationOnce(async () => { started.resolve(); await release.promise; return { error: 'save refused' } })
  const running = hook.startUpload(bundle().files, '/models')
  await started.promise
  const progress = hook.uploadProgress.value
  emitter.emit('upload_progress', { id: 'other', uploadedSize: 9999 } as IUploadItem)
  expect(hook.uploadProgress.value).toBe(progress)
  release.resolve()
  await running
  expect(io.merge).toHaveBeenCalledOnce()
  expect(hook.uploadDone.value).toBe(false)
  expect(io.toast).toHaveBeenCalledWith(expect.stringContaining('save refused'), 'error')
})
