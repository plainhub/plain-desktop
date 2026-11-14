import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { IUploadItem } from '@/stores/temp'

const state = vi.hoisted(() => ({ peer: 'peer-a', revisions: new Map<string, number>() }))
const io = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: io.invoke }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/device/client-id', () => ({ getActiveClientId: () => state.peer, getRemoteClientId: () => state.peer }))
vi.mock('@/lib/device/login-peers', () => ({ getLoginPeerRevision: (id: string) => state.revisions.get(id) ?? 0 }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => { throw new Error('Desktop must not read token') } }))
vi.mock('@/lib/api/api', () => ({ getApiHeaders: () => ({ 'c-id': 'self' }) }))
vi.mock('@/lib/api/crypto', () => ({}))
vi.mock('@/lib/api/file', () => ({}))
vi.mock('@/lib/api/time-sync', () => ({}))
vi.mock('@/lib/api/query', () => ({ uploadedChunksGQL: 'chunks' }))
vi.mock('@/lib/api/mutation', () => ({ deleteChunksGQL: 'delete' }))
vi.mock('@/lib/upload/merge-async', () => ({}))

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(done => { resolve = done })
  return { promise, resolve }
}
beforeEach(() => {
  vi.resetModules(); vi.resetAllMocks()
  state.peer = 'peer-a'
  state.revisions.clear(); state.revisions.set('peer-a', 1)
  vi.stubGlobal('__IS_TAURI__', true)
  vi.stubGlobal('window', { __PLAIN_LOG__: false })
})
afterEach(() => vi.unstubAllGlobals())

it.each(['auth-change', 'selection-change'])('rechecks the original peer after the native scope wait: %s', async change => {
  const scope = deferred<string>()
  const started = deferred<void>()
  io.invoke.mockImplementation(async command => {
    if (command === 'native_view_scope') { started.resolve(); return scope.promise }
    return { data: { deleteChunks: true } }
  })
  const { discardUploadChunks } = await import('@/lib/upload/upload')
  const { getUploadContext } = await import('@/lib/upload/context')
  const item: IUploadItem = { id: 'old-upload', fileId: 'file-id', file: new File(['data'], 'sample.bin'), fileName: 'sample.bin', dir: '/owned', status: 'canceled', uploadedSize: 0, error: '' }
  getUploadContext(item)
  const cleanup = discardUploadChunks(item, () => false)
  const outcome = cleanup.then(() => undefined, error => error)
  await started.promise
  if (change === 'auth-change') state.revisions.set('peer-a', 2)
  else state.peer = 'peer-b'
  scope.resolve('original-document')
  const error = await outcome
  if (change === 'auth-change') {
    expect(error).toMatchObject({ message: 'request_context_changed', canceled: true })
    expect(io.invoke.mock.calls).toEqual([['native_view_scope']])
  } else {
    expect(error).toBeUndefined()
    expect(io.invoke.mock.calls).toEqual([
      ['native_view_scope'],
      ['native_graphql', { target: { kind: 'peer', peerId: 'peer-a' }, body: { query: 'delete', variables: { fileId: 'file-id' }, extensions: { nativeScope: 'original-document', nativeDeadlineMs: expect.any(Number) } } }],
    ])
  }
})
