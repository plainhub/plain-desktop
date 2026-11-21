import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { IFile } from '@/lib/file'
import { FILE_TRASH_COLLAPSE_MS, useFilesTrash } from '@/views/files/hooks/useFilesTrash'

vi.mock('@/components/toaster', () => ({ default: vi.fn() }))

const files = [{ id: 'one', path: '/files/one', name: 'one' }, { id: 'two', path: '/files/two', name: 'two' }] as IFile[]
const fixture = () => {
  const mutate = vi.fn()
  const onDeleted = vi.fn()
  const refresh = vi.fn()
  return { mutate, onDeleted, refresh, ...useFilesTrash({ t: (key) => key, mutate, onDeleted, refresh }) }
}
beforeEach(() => vi.useFakeTimers())
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers() })

it('sends all selected paths once and removes rows after successful completion', async () => {
  const view = fixture()
  view.mutate.mockResolvedValue({ data: { trashFiles: { affectedCount: 2 } } })
  await view.startTrash(files, { bulk: true })
  expect(view.mutate).toHaveBeenCalledExactlyOnceWith({ paths: ['/files/one', '/files/two'] })
  expect(view.onDeleted).not.toHaveBeenCalled()
  expect(view.collapsingIds.value).toEqual(['one', 'two'])
  await vi.advanceTimersByTimeAsync(FILE_TRASH_COLLAPSE_MS)
  expect(view.onDeleted).toHaveBeenCalledExactlyOnceWith(files)
  expect(view.refresh).not.toHaveBeenCalled()
  expect(view.trashingIds.value).toEqual([])
  expect(view.collapsingIds.value).toEqual([])
})

it('refreshes actual state on a GraphQL failure without removing every selected row', async () => {
  const view = fixture()
  view.mutate.mockResolvedValue(undefined)
  await view.startTrash(files, { bulk: true })
  expect(view.refresh).toHaveBeenCalledTimes(1)
  expect(view.onDeleted).not.toHaveBeenCalled()
  expect(view.trashingIds.value).toEqual([])
  expect(view.collapsingIds.value).toEqual([])
  expect(view.bulkTrashing.value).toBe(false)
  expect(vi.getTimerCount()).toBe(0)
})

it('refreshes after a transport failure whose physical effects are unknown', async () => {
  const view = fixture()
  view.mutate.mockRejectedValue(new Error('connection lost'))
  await view.startTrash(files)
  expect(view.refresh).toHaveBeenCalledTimes(1)
  expect(view.onDeleted).not.toHaveBeenCalled()
  expect(view.trashingIds.value).toEqual([])
})

it('does not request deletion or refresh for an empty selection', async () => {
  const view = fixture()
  await view.startTrash([], { bulk: true })
  expect(view.mutate).not.toHaveBeenCalled()
  expect(view.refresh).not.toHaveBeenCalled()
  expect(view.bulkTrashing.value).toBe(false)
})
