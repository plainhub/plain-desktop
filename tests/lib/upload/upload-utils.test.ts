import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

/**
 * Tests for upload utility functions: generateFileId, createChunk, getMD5Hash.
 * We import the public helpers and test their behaviour in isolation.
 */

// Mock the modules that upload.ts depends on before importing
const { proxyUrlForMock } = vi.hoisted(() => ({
  proxyUrlForMock: vi.fn((base: string, path: string) => `${base}${path}`),
}))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: vi.fn() } }))
vi.mock('@/lib/api/api', () => ({
  getApiBaseUrl: () => 'http://localhost:3000',
  proxyUrlFor: proxyUrlForMock,
  getLocalToken: () => '',
  getApiHeaders: () => ({ 'Content-Type': 'multipart/form-data' }),
}))
vi.mock('@/lib/api/crypto', () => ({
  chachaEncrypt: (_key: Uint8Array, data: string) => new Uint8Array(new TextEncoder().encode(data)),
  bitArrayToUint8Array: (arr: Uint8Array) => arr,
}))
vi.mock('@/lib/api/file', () => ({
  tokenToKey: () => new Uint8Array(32),
}))
vi.mock('@/lib/api/query', () => ({
  uploadedChunksGQL: 'query uploadedChunks($fileId: String!) { uploadedChunks(fileId: $fileId) }',
}))
vi.mock('@/lib/api/mutation', () => ({
  mergeChunksGQL: 'mutation mergeChunks { mergeChunks }',
  deleteChunksGQL: 'mutation deleteChunks($fileId: String!) { deleteChunks(fileId: $fileId) }',
}))
vi.mock('@/lib/api/gql-client', () => ({
  gqlFetch: vi.fn(),
}))

import { generateFileId, getMD5Hash, getUploadUrl, getUploadChunkUrl } from '@/lib/upload/upload'

describe('getUploadUrl', () => {
  // Upload URLs must go through proxyUrlFor: in Tauri remote mode that is
  // what embeds the `_pt` proxy target — a bare base+path URL left the
  // local HTTP proxy without a target and it answered 400.
  it('builds the URL through proxyUrlFor so the proxy target rides along', () => {
    proxyUrlForMock.mockClear()
    expect(getUploadUrl()).toBe('http://localhost:3000/upload')
    expect(proxyUrlForMock).toHaveBeenCalledWith('http://localhost:3000', '/upload')
  })
})

describe('getUploadChunkUrl', () => {
  it('builds the chunk URL through proxyUrlFor', () => {
    proxyUrlForMock.mockClear()
    expect(getUploadChunkUrl()).toBe('http://localhost:3000/upload_chunk')
    expect(proxyUrlForMock).toHaveBeenCalledWith('http://localhost:3000', '/upload_chunk')
  })
})

describe('getMD5Hash', () => {
  it('returns a 32-char hex string', async () => {
    const data = new TextEncoder().encode('hello world').buffer as ArrayBuffer
    const hash = await getMD5Hash(data)
    expect(hash).toHaveLength(32)
    expect(hash).toMatch(/^[0-9a-f]{32}$/)
  })

  it('returns consistent hash for same input', async () => {
    const data = new TextEncoder().encode('test data').buffer as ArrayBuffer
    const hash1 = await getMD5Hash(data)
    const hash2 = await getMD5Hash(data)
    expect(hash1).toBe(hash2)
  })

  it('returns different hashes for different inputs', async () => {
    const data1 = new TextEncoder().encode('input A').buffer as ArrayBuffer
    const data2 = new TextEncoder().encode('input B').buffer as ArrayBuffer
    const hash1 = await getMD5Hash(data1)
    const hash2 = await getMD5Hash(data2)
    expect(hash1).not.toBe(hash2)
  })

  it('handles empty input', async () => {
    const data = new ArrayBuffer(0)
    const hash = await getMD5Hash(data)
    expect(hash).toHaveLength(32)
    expect(hash).toMatch(/^[0-9a-f]{32}$/)
  })
})

describe('generateFileId', () => {
  function createMockFile(name: string, size: number, lastModified: number): File {
    // Create a file with deterministic content
    const content = new Uint8Array(size)
    for (let i = 0; i < size; i++) {
      content[i] = i % 256
    }
    const file = new File([content], name, { lastModified })
    return file
  }

  it('returns a 32-char hex string', async () => {
    const file = createMockFile('test.txt', 1024, 1000000)
    const id = await generateFileId(file)
    expect(id).toHaveLength(32)
    expect(id).toMatch(/^[0-9a-f]{32}$/)
  })

  it('returns same ID for same file', async () => {
    const file1 = createMockFile('test.txt', 1024, 1000000)
    const file2 = createMockFile('test.txt', 1024, 1000000)
    const id1 = await generateFileId(file1)
    const id2 = await generateFileId(file2)
    expect(id1).toBe(id2)
  })

  it('returns different ID for different name', async () => {
    const file1 = createMockFile('a.txt', 1024, 1000000)
    const file2 = createMockFile('b.txt', 1024, 1000000)
    const id1 = await generateFileId(file1)
    const id2 = await generateFileId(file2)
    expect(id1).not.toBe(id2)
  })

  it('returns different ID for different size', async () => {
    const file1 = createMockFile('test.txt', 1024, 1000000)
    const file2 = createMockFile('test.txt', 2048, 1000000)
    const id1 = await generateFileId(file1)
    const id2 = await generateFileId(file2)
    expect(id1).not.toBe(id2)
  })

  it('returns different ID for different lastModified', async () => {
    const file1 = createMockFile('test.txt', 1024, 1000000)
    const file2 = createMockFile('test.txt', 1024, 2000000)
    const id1 = await generateFileId(file1)
    const id2 = await generateFileId(file2)
    expect(id1).not.toBe(id2)
  })

  it('handles large files (only reads first 2MB)', async () => {
    // 10MB file — should still generate an ID using only the first 2MB
    const file = createMockFile('large.bin', 10 * 1024 * 1024, 1000000)
    const id = await generateFileId(file)
    expect(id).toHaveLength(32)
    expect(id).toMatch(/^[0-9a-f]{32}$/)
  })
})
