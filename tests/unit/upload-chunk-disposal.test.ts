import { beforeEach, expect, it, vi } from 'vitest'
import type { IUploadItem } from '@/stores/temp'
const io = vi.hoisted(() => ({ gql: vi.fn(), emit: vi.fn() }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/api/time-sync', () => ({ wrapWithReplayProtection: (value: string) => value }))
vi.mock('@/lib/api/api', () => ({ getApiBaseUrl: vi.fn(), getLocalToken: vi.fn(), proxyUrlFor: vi.fn() }))
vi.mock('@/lib/api/crypto', () => ({ chachaEncrypt: vi.fn(), bitArrayToUint8Array: vi.fn() }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: () => new Uint8Array(32) }))
vi.mock('@/lib/api/query', () => ({ uploadedChunksGQL: 'query-chunks' }))
vi.mock('@/lib/api/mutation', () => ({ deleteChunksGQL: 'delete-chunks' }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: io.gql, GqlError: class extends Error {} }))
vi.mock('@/lib/upload/merge-async', () => ({ requestMerge: vi.fn() }))
vi.mock('@/lib/upload/batch-progress', () => ({ setUploadStatus: (item: IUploadItem, status: string) => { item.status = status }, setUploadUploadedSize: (item: IUploadItem, size: number) => { item.uploadedSize = size } }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => 'test-token' }))
vi.mock('@/lib/device/client-id', () => ({ getDesktopClientId: () => 'test-client' }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: io.emit } }))
import { upload, discardUploadChunks } from '@/lib/upload/upload'
import { GqlError } from '@/lib/api/gql-client'
const fileId = 'a'.repeat(32)
function item(status = 'uploading'): IUploadItem {
  return { id: 'test', fileId, fileName: 'large.bin', file: new File([new Uint8Array(12 * 1024 * 1024)], 'large.bin'), dir: '/owned', uploadedSize: 0, status, error: '' }
}
beforeEach(() => { io.gql.mockReset().mockResolvedValue({ data: { deleteChunks: true } }); io.emit.mockReset() })
it('waits for the actual chunk operation before deleting and stops canceled progress', async () => {
  let finish!: (value: unknown) => void
  io.gql.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const current = item()
  const running = upload(current, false)
  await vi.waitFor(() => expect(io.gql).toHaveBeenCalledWith('query-chunks', { fileId }, { timeout: 120000 }))
  current.status = 'canceled'
  const cleanup = discardUploadChunks(fileId, () => false)
  expect(io.gql).toHaveBeenCalledTimes(1)
  finish({ data: { uploadedChunks: [] } })
  await Promise.all([running, cleanup])
  expect(io.gql).toHaveBeenCalledTimes(2)
  expect(io.gql).toHaveBeenLastCalledWith('delete-chunks', { fileId })
  expect(io.emit).not.toHaveBeenCalled()
})
it('checks other owners after acquiring the existing file serial boundary', async () => {
  let finish!: (value: unknown) => void
  io.gql.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const first = discardUploadChunks(fileId, () => false)
  await vi.waitFor(() => expect(io.gql).toHaveBeenCalledOnce())
  let referenced = false
  const second = discardUploadChunks(fileId, () => referenced)
  referenced = true
  finish({ data: { deleteChunks: true } })
  await Promise.all([first, second])
  expect(io.gql).toHaveBeenCalledOnce()
})
it('does not start requests for a canceled task waiting for its file turn', async () => {
  await upload(item('canceled'), false)
  expect(io.gql).not.toHaveBeenCalled()
})
it.each(['graphql', 'network'])('rejects an actual %s cleanup error', async mode => {
  if (mode === 'graphql') io.gql.mockResolvedValueOnce({ errors: [{ message: 'Permission denied' }] })
  else io.gql.mockRejectedValueOnce(new Error('Disconnected'))
  const result = discardUploadChunks(fileId, () => false)
  if (mode === 'graphql') await expect(result).rejects.toBeInstanceOf(GqlError)
  else await expect(result).rejects.toThrow('Disconnected')
})
