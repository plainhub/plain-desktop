import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'

vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string, args?: any) => key }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))

import toast from '@/components/toaster'
import { FILE_DELETE_COLLAPSE_MS, useFilesDelete } from '@/views/files/hooks/useFilesDelete'
import type { IFile } from '@/lib/file'

const file = (id: string, name = id): IFile => ({ id, name, path: `/${name}` }) as IFile

const deferred = () => {
  let resolve!: (value: any) => void
  const promise = new Promise<any>((r) => { resolve = r })
  return { promise, resolve }
}

const setup = (mutate = vi.fn(() => Promise.resolve({}))) => {
  const onDeleted = vi.fn()
  const t = (key: string, args?: any) => (args ? `${key}:${JSON.stringify(args)}` : key)
  const flow = useFilesDelete({ t, mutate, onDeleted })
  return { ...flow, mutate, onDeleted }
}

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
  vi.clearAllMocks()
})

describe('useFilesDelete', () => {
  it('moves ids deleting → collapsing → removed on success and toasts', async () => {
    const d = deferred()
    const flow = setup(vi.fn(() => d.promise))

    const done = flow.startDelete([file('a'), file('b')])
    expect(flow.deletingIds.value).toEqual(['a', 'b'])
    expect(flow.bulkDeleting.value).toBe(false)

    d.resolve({})
    await done
    expect(flow.deletingIds.value).toEqual([])
    expect(flow.collapsingIds.value).toEqual(['a', 'b'])
    expect(flow.onDeleted).not.toHaveBeenCalled()
    expect(toast).toHaveBeenCalledWith('deleted_n_items:{"n":2}')

    vi.advanceTimersByTime(FILE_DELETE_COLLAPSE_MS)
    expect(flow.onDeleted).toHaveBeenCalledTimes(1)
    expect(flow.collapsingIds.value).toEqual([])
  })

  it('toasts the deleted key for a single-file delete', async () => {
    const flow = setup()
    await flow.startDelete([file('a')])
    expect(toast).toHaveBeenCalledWith('deleted')
  })

  it('restores ids and toasts the failure message when the mutation fails', async () => {
    const d = deferred()
    const flow = setup(vi.fn(() => d.promise))

    const done = flow.startDelete([file('a', 'photo.jpg')])
    expect(flow.deletingIds.value).toEqual(['a'])

    d.resolve(undefined)
    await done
    expect(flow.deletingIds.value).toEqual([])
    expect(flow.collapsingIds.value).toEqual([])
    expect(flow.onDeleted).not.toHaveBeenCalled()
    expect(toast).toHaveBeenCalledWith('delete_failed_name:{"name":"photo.jpg"}', 'error')

    vi.advanceTimersByTime(FILE_DELETE_COLLAPSE_MS)
    expect(flow.onDeleted).not.toHaveBeenCalled()
  })

  it('flags bulkDeleting while a bulk delete is in flight only', async () => {
    const d = deferred()
    const flow = setup(vi.fn(() => d.promise))

    const done = flow.startDelete([file('a')], { bulk: true })
    expect(flow.bulkDeleting.value).toBe(true)

    d.resolve({})
    await done
    expect(flow.bulkDeleting.value).toBe(false)
  })

  it('reports deleting for in-flight and collapsing ids and ignores empty batches', async () => {
    const d = deferred()
    const flow = setup(vi.fn(() => d.promise))

    const done = flow.startDelete([file('a')])
    expect(flow.isDeleting('a')).toBe(true)

    d.resolve({})
    await done
    expect(flow.isDeleting('a')).toBe(true)

    vi.advanceTimersByTime(FILE_DELETE_COLLAPSE_MS)
    expect(flow.isDeleting('a')).toBe(false)

    await flow.startDelete([])
    expect(flow.mutate).toHaveBeenCalledTimes(1)
  })

  it('keeps unrelated deleting ids when one batch settles', async () => {
    const first = deferred()
    const second = deferred()
    const mutate = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    const flow = setup(mutate)

    const batchA = flow.startDelete([file('a')])
    const batchB = flow.startDelete([file('b')])
    expect(flow.deletingIds.value).toEqual(['a', 'b'])

    first.resolve(undefined)
    await batchA
    expect(flow.deletingIds.value).toEqual(['b'])

    second.resolve({})
    await batchB
    expect(flow.deletingIds.value).toEqual([])
    expect(flow.collapsingIds.value).toEqual(['b'])
  })
})
