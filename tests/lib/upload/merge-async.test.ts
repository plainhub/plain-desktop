import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { IUploadItem } from '@/stores/temp'
import type { GqlResult } from '@/lib/api/gql-client'

const mockGqlFetch = vi.fn()
vi.mock('@/lib/api/gql-client', () => ({
  gqlFetch: (...args: any[]) => mockGqlFetch(...args),
  GqlError: class GqlError extends Error {},
}))

import { requestMerge } from '@/lib/upload/merge-async'
import { mergeChunksGQL, mergeAppFileChunksGQL } from '@/lib/api/mutation'
import { mergeStatusGQL } from '@/lib/api/query'
import emitter from '@/plugins/eventbus'

function uploadItem(overrides: Partial<IUploadItem> = {}): IUploadItem {
  return {
    id: 'u1',
    dir: '/downloads',
    fileName: 'big.bin',
    file: { size: 50 * 1024 * 1024 } as unknown as File,
    uploadedSize: 0,
    status: 'uploading',
    error: '',
    ...overrides,
  }
}

const MERGE_ARGS = { fileId: 'f1', totalChunks: 5, path: '/downloads/big.bin', replace: true, totalSize: 50 * 1024 * 1024 }

describe('requestMerge', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('resolves via the WS event after STARTED', async () => {
    mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunks: { status: 'STARTED' } } } as GqlResult)
    const pending = requestMerge(uploadItem(), MERGE_ARGS)
    await new Promise((r) => setTimeout(r, 0))
    emitter.emit('upload_merge_result', { fileId: 'other', ok: true, value: 'x', mergedSize: 1 })
    emitter.emit('upload_merge_result', { fileId: 'f1', ok: true, value: 'big.bin', mergedSize: 50 * 1024 * 1024 })
    await expect(pending).resolves.toEqual({ value: 'big.bin', size: 50 * 1024 * 1024 })
    expect(mockGqlFetch).toHaveBeenCalledTimes(1)
    expect(mockGqlFetch.mock.calls[0][0]).toBe(mergeChunksGQL)
  })

  it('returns immediately when the merge already finished (DONE task)', async () => {
    mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunks: { status: 'DONE', value: 'big.bin', mergedSize: 52428800 } } } as GqlResult)
    await expect(requestMerge(uploadItem(), MERGE_ARGS)).resolves.toEqual({ value: 'big.bin', size: 52428800 })
  })

  it('routes app-file uploads to mergeAppFileChunks', async () => {
    mockGqlFetch.mockResolvedValueOnce({ data: { mergeAppFileChunks: { status: 'DONE', value: 'pic.png', mergedSize: 3 } } } as GqlResult)
    await expect(
      requestMerge(uploadItem({ isAppFile: true }), { fileId: 'f9', totalChunks: 1, fileName: 'pic.png', totalSize: 3 }),
    ).resolves.toEqual({ value: 'pic.png', size: 3 })
    expect(mockGqlFetch.mock.calls[0][0]).toBe(mergeAppFileChunksGQL)
  })

  it('propagates merge failures from the task', async () => {
    mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunks: { status: 'FAILED', error: 'Merge integrity failed' } } } as GqlResult)
    await expect(requestMerge(uploadItem(), MERGE_ARGS)).resolves.toEqual({ error: 'Merge integrity failed' })
  })

  it('surfaces mutation-level GraphQL errors', async () => {
    mockGqlFetch.mockResolvedValueOnce({ data: null, errors: [{ message: 'No chunks found for f1' }] } as GqlResult)
    await expect(requestMerge(uploadItem(), MERGE_ARGS)).resolves.toEqual({ error: 'No chunks found for f1' })
  })

  it('recovers a lost WS event via the mergeStatus watchdog', async () => {
    vi.useFakeTimers()
    try {
      mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunks: { status: 'MERGING' } } } as GqlResult)
      const pending = requestMerge(uploadItem(), MERGE_ARGS)
      mockGqlFetch.mockResolvedValueOnce({ data: { mergeStatus: { status: 'MERGING' } } } as GqlResult)
      await vi.advanceTimersByTimeAsync(15_500)
      mockGqlFetch.mockResolvedValueOnce({ data: { mergeStatus: { status: 'DONE', value: 'big.bin', mergedSize: 52428800 } } } as GqlResult)
      await vi.advanceTimersByTimeAsync(15_500)
      await expect(pending).resolves.toEqual({ value: 'big.bin', size: 52428800 })
      const statusCalls = mockGqlFetch.mock.calls.filter((c) => c[0] === mergeStatusGQL)
      expect(statusCalls).toHaveLength(2)
    } finally {
      vi.useRealTimers()
    }
  })

  it('stops waiting when the upload is paused mid-merge', async () => {
    vi.useFakeTimers()
    try {
      const item = uploadItem()
      mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunks: { status: 'STARTED' } } } as GqlResult)
      const pending = requestMerge(item, MERGE_ARGS)
      item.status = 'paused'
      await vi.advanceTimersByTimeAsync(1_100)
      await expect(pending).resolves.toEqual({ error: 'Upload paused' })
    } finally {
      vi.useRealTimers()
    }
  })

  it('fails with connection_timeout after exhausting the poll budget', async () => {
    vi.useFakeTimers()
    try {
      mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunks: { status: 'STARTED' } } } as GqlResult)
      mockGqlFetch.mockResolvedValue({ data: { mergeStatus: { status: 'MERGING' } } } as GqlResult)
      const pending = requestMerge(uploadItem({ file: { size: 10 * 1024 * 1024 } as unknown as File }), MERGE_ARGS)
      // 60s + 5s budget at 15s per poll → 5 polls
      await vi.advanceTimersByTimeAsync(120_000)
      await expect(pending).resolves.toEqual({ error: 'connection_timeout' })
    } finally {
      vi.useRealTimers()
    }
  })
})
