import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { IUploadItem } from '@/stores/temp'
import type { GqlResult } from '@/lib/api/gql-client'

const mockGqlFetch = vi.fn()
vi.mock('@/lib/api/gql-client', () => ({
  gqlFetch: (...args: any[]) => mockGqlFetch(...args),
  GqlError: class GqlError extends Error {},
}))

import { parseMergeState, requestMerge, resetAsyncMergeCapabilityForTests } from '@/lib/upload/merge-async'
import { mergeChunksAsyncGQL, mergeChunksGQL } from '@/lib/api/mutation'
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

const MERGE_ARGS = { fileId: 'f1', totalChunks: 5, path: '/downloads/big.bin', replace: true, isAppFile: false, totalSize: 50 * 1024 * 1024 }

describe('parseMergeState', () => {
  it('parses the plain states', () => {
    expect(parseMergeState('started')).toEqual({ state: 'started' })
    expect(parseMergeState('merging')).toEqual({ state: 'merging' })
    expect(parseMergeState('none')).toEqual({ state: 'none' })
    expect(parseMergeState('')).toEqual({ state: 'none' })
  })

  it('parses done/failed, tolerating colons in the value (splits from the right)', () => {
    expect(parseMergeState('done:a:b.jpg:42')).toEqual({ state: 'done', value: 'a:b.jpg', size: 42 })
    expect(parseMergeState('done:v:0')).toEqual({ state: 'done', value: 'v', size: 0 })
    expect(parseMergeState('failed:boom: detail')).toEqual({ state: 'failed', error: 'boom: detail' })
  })
})

describe('requestMerge', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resetAsyncMergeCapabilityForTests()
  })

  it('resolves via the WS event after "started"', async () => {
    mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunksAsync: 'started' } } as GqlResult)
    const pending = requestMerge(uploadItem(), MERGE_ARGS)
    await new Promise((r) => setTimeout(r, 0))
    emitter.emit('upload_merge_result', { fileId: 'other', ok: true, value: 'x', mergedSize: 1 })
    emitter.emit('upload_merge_result', { fileId: 'f1', ok: true, value: 'big.bin', mergedSize: 50 * 1024 * 1024 })
    await expect(pending).resolves.toEqual({ value: 'big.bin', size: 50 * 1024 * 1024 })
    expect(mockGqlFetch).toHaveBeenCalledTimes(1)
    expect(mockGqlFetch.mock.calls[0][0]).toBe(mergeChunksAsyncGQL)
  })

  it('returns immediately when the merge already finished ("done:…")', async () => {
    mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunksAsync: 'done:big.bin:52428800' } } as GqlResult)
    await expect(requestMerge(uploadItem(), MERGE_ARGS)).resolves.toEqual({ value: 'big.bin', size: 52428800 })
  })

  it('falls back to the sync mutation once for legacy servers, then remembers', async () => {
    mockGqlFetch.mockResolvedValueOnce({ data: null, errors: [{ message: 'unknown field `mergeChunksAsync` on type `Mutation`' }] } as GqlResult)
    mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunks: 'big.bin:52428800' } } as GqlResult)
    await expect(requestMerge(uploadItem(), MERGE_ARGS)).resolves.toEqual({ value: 'big.bin', size: 52428800 })
    expect(mockGqlFetch.mock.calls[1][0]).toBe(mergeChunksGQL)

    mockGqlFetch.mockClear()
    mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunks: 'again.bin:1' } } as GqlResult)
    await requestMerge(uploadItem(), { ...MERGE_ARGS, fileId: 'f2' })
    expect(mockGqlFetch).toHaveBeenCalledTimes(1)
    expect(mockGqlFetch.mock.calls[0][0]).toBe(mergeChunksGQL)
  })

  it('propagates merge failures from the async mutation', async () => {
    mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunksAsync: 'failed:Merge integrity failed' } } as GqlResult)
    await expect(requestMerge(uploadItem(), MERGE_ARGS)).resolves.toEqual({ error: 'Merge integrity failed' })
  })

  it('recovers a lost WS event via the mergeStatus watchdog', async () => {
    vi.useFakeTimers()
    try {
      mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunksAsync: 'merging' } } as GqlResult)
      const pending = requestMerge(uploadItem(), MERGE_ARGS)
      mockGqlFetch.mockResolvedValueOnce({ data: { mergeStatus: 'merging' } } as GqlResult)
      await vi.advanceTimersByTimeAsync(15_500)
      mockGqlFetch.mockResolvedValueOnce({ data: { mergeStatus: 'done:big.bin:52428800' } } as GqlResult)
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
      mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunksAsync: 'started' } } as GqlResult)
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
      mockGqlFetch.mockResolvedValueOnce({ data: { mergeChunksAsync: 'started' } } as GqlResult)
      mockGqlFetch.mockResolvedValue({ data: { mergeStatus: 'merging' } } as GqlResult)
      const pending = requestMerge(uploadItem({ file: { size: 10 * 1024 * 1024 } as unknown as File }), MERGE_ARGS)
      // 60s + 5s budget at 15s per poll → 5 polls
      await vi.advanceTimersByTimeAsync(120_000)
      await expect(pending).resolves.toEqual({ error: 'connection_timeout' })
    } finally {
      vi.useRealTimers()
    }
  })
})
