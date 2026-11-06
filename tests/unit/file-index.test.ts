import { beforeEach, expect, it, vi } from 'vitest'
import { saveFileIndexVolume, rebuildFileIndexVolume, canScanVolume, type FileIndexVolume } from '@/lib/api/file-index'

const request = vi.hoisted(() => vi.fn())
vi.mock('@/lib/api/gql-client', () => ({ bindGraphqlClient: () => request }))
beforeEach(() => request.mockReset())

const volume: FileIndexVolume = {
  id: 'volume-a',
  path: '/disk',
  isAvailable: true,
  indexMode: 'BLOCKLIST',
  includedDirs: ['/disk/Photos'],
  excludedDirs: ['/disk/Cache'],
}

it('saves both lists and the selected mode in one main-schema mutation', async () => {
  request.mockResolvedValue({ data: { setFileIndexVolume: true } })
  await saveFileIndexVolume(volume)
  expect(request).toHaveBeenCalledOnce()
  expect(request.mock.calls[0][1]).toEqual({
    id: volume.id,
    indexMode: 'BLOCKLIST',
    includedDirs: ['/disk/Photos'],
    excludedDirs: ['/disk/Cache'],
  })
})

it('reports refusals and GraphQL failures to the editable dialog', async () => {
  request.mockResolvedValueOnce({ data: { setFileIndexVolume: false } })
  await expect(saveFileIndexVolume(volume)).rejects.toThrow('Index configuration write refused')
  request.mockResolvedValueOnce({ errors: [{ message: 'Directory belongs to another volume' }] })
  await expect(saveFileIndexVolume(volume)).rejects.toThrow('Directory belongs to another volume')
})

it('rebuilds one volume by its stable ID instead of a root-path sentinel', async () => {
  request.mockResolvedValue({ data: { rebuildFileIndexVolume: true } })
  await rebuildFileIndexVolume(volume.id)
  expect(request).toHaveBeenCalledWith(expect.stringContaining('rebuildFileIndexVolume'), { id: volume.id })
})

it('offers per-volume indexing only for online configured scopes', () => {
  expect(canScanVolume(volume)).toBe(true)
  expect(canScanVolume({ ...volume, isAvailable: false })).toBe(false)
  expect(canScanVolume({ ...volume, indexMode: 'UNCONFIGURED' })).toBe(false)
  expect(canScanVolume({ ...volume, indexMode: 'DISABLED' })).toBe(false)
  expect(canScanVolume({ ...volume, indexMode: 'ALLOWLIST', includedDirs: [] })).toBe(false)
  expect(canScanVolume({ ...volume, indexMode: 'ALLOWLIST' })).toBe(true)
})
