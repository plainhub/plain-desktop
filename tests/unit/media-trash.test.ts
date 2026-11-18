import { beforeEach, expect, it, vi } from 'vitest'
import { DataType } from '@/lib/data'
import { useMediaRestore, useMediaTrash } from '@/hooks/media-trash'
import { gqlFetch } from '@/lib/api/gql-client'
import { restoreMediaItemsGQL, trashMediaItemsGQL } from '@/lib/api/mutation'
import emitter from '@/plugins/eventbus'

vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: vi.fn() } }))

beforeEach(() => vi.clearAllMocks())

it('leaves a conflicting restore in Trash and lets the user try again', async () => {
  const { restore, restoreLoading } = useMediaRestore()
  vi.mocked(gqlFetch).mockResolvedValueOnce({ errors: [{ message: 'The original location already contains a file' }] })
  await expect(restore(DataType.IMAGE, 'ids:1')).resolves.toBe(false)
  expect(restoreLoading('ids:1')).toBe(false)
  expect(emitter.emit).not.toHaveBeenCalledWith('media_items_actioned', expect.anything())
  vi.mocked(gqlFetch).mockResolvedValueOnce({ data: { restoreMediaItems: { affectedCount: 1 } } })
  await expect(restore(DataType.IMAGE, 'ids:1')).resolves.toBe(true)
  expect(gqlFetch).toHaveBeenLastCalledWith(restoreMediaItemsGQL, { type: DataType.IMAGE, query: 'ids:1' }, { dedupe: false })
  expect(emitter.emit).toHaveBeenCalledWith('media_items_actioned', { type: DataType.IMAGE, action: 'restore', query: 'ids:1' })
})

it('refreshes instead of removing every selected row when only part of a move succeeded', async () => {
  vi.mocked(gqlFetch).mockResolvedValueOnce({ data: { trashMediaItems: { affectedCount: 1 } } })
  const { trash } = useMediaTrash()
  await expect(trash(DataType.IMAGE, 'ids:1,2')).resolves.toBe(true)
  expect(gqlFetch).toHaveBeenCalledExactlyOnceWith(trashMediaItemsGQL, { type: DataType.IMAGE, query: 'ids:1,2' }, { dedupe: false })
  expect(emitter.emit).toHaveBeenCalledWith('media_items_actioned', { type: DataType.IMAGE, action: 'trash', query: '' })
})

it('uses the existing Trash capability only for the local image gallery', async () => {
  const { hasMediaTrash } = await import('@/lib/feature')
  const { Capability } = await import('@/lib/data')
  const { setRemoteClientId, clearRemoteClientId } = await import('@/lib/device/client-id')
  vi.stubGlobal('__IS_TAURI__', true)
  const app = { capabilities: [Capability.MEDIA_TRASH] }
  try {
    clearRemoteClientId()
    expect(hasMediaTrash(app, DataType.IMAGE)).toBe(true)
    for (const type of [DataType.VIDEO, DataType.AUDIO, DataType.DOC]) {
      expect(hasMediaTrash(app, type)).toBe(false)
    }
    setRemoteClientId('fixture-remote')
    expect(hasMediaTrash(app, DataType.VIDEO)).toBe(true)
    expect(hasMediaTrash({ capabilities: [] }, DataType.IMAGE)).toBe(false)
  } finally {
    clearRemoteClientId()
    vi.unstubAllGlobals()
  }
})

it('identifies local trashed image previews without blocking lookalike folders', async () => {
  const { isTrashedMediaPath } = await import('@/lib/media/trash-path')
  expect(isTrashedMediaPath('/volume/.plain-trash/album/image.png')).toBe(true)
  expect(isTrashedMediaPath(String.raw`C:\volume\.plain-trash\album\image.png`)).toBe(true)
  expect(isTrashedMediaPath('/volume/album/.trashed-123-image.png')).toBe(true)
  expect(isTrashedMediaPath('/volume/album.plain-trash/image.png')).toBe(false)
  expect(isTrashedMediaPath('/volume/album/scan.trashed-123.png')).toBe(false)
})
