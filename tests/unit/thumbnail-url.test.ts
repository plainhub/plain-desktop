import { expect, it, vi } from 'vitest'
vi.mock('@/lib/api/file', () => ({
  getFileId: (_key: unknown, path: string) => path,
  getFileUrl: (id: string, query: string) => `/fs?id=${encodeURIComponent(id)}${query}`,
  getFileName: vi.fn(), getFileExtension: vi.fn(),
}))
import { fileThumbUrl } from '@/lib/file'

it('changes file thumbnail URLs when the image modification timestamp changes', () => {
  const image = { path: '/photo.jpg', name: 'photo.jpg', isDir: false, updatedAt: '2026-01-01T00:00:00.001Z' }
  const before = fileThumbUrl(null, image)
  const after = fileThumbUrl(null, { ...image, updatedAt: '2026-01-01T00:00:00.002Z' })
  expect(after).not.toBe(before)
  expect(after).toContain('&s=sm&v=2026-01-01T00%3A00%3A00.002Z')
  expect(fileThumbUrl(null, { ...image, isDir: true })).toBe('')
  expect(fileThumbUrl(null, { ...image, name: 'document.txt' })).toBe('')
})
