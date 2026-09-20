import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import type { DataType } from '@/lib/data'

const harness = vi.hoisted(() => ({
  mutates: [] as ReturnType<typeof vi.fn>[],
}))

vi.mock('vue-i18n', async (importOriginal) => {
  const actual = await importOriginal<typeof import('vue-i18n')>()
  return { ...actual, useI18n: () => ({ t: (key: string) => key }) }
})
vi.mock('@/components/toaster', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/components/toaster')>()
  return { ...actual, default: vi.fn() }
})
vi.mock('@/lib/api/mutation', async (importOriginal) => {
  const { ref } = await vi.importActual<typeof import('vue')>('vue')
  const actual = await importOriginal<typeof import('@/lib/api/mutation')>()
  return {
    ...actual,
    initMutation: () => {
      const mutate = vi.fn()
      harness.mutates.push(mutate)
      return { mutate, loading: ref(false), onDone: vi.fn() }
    },
  }
})

import { selectAllQuery, useDelete } from '@/hooks/list'
import { useDeleteItems, useMoveItems } from '@/hooks/media'
import { useOrganizeUndo } from '@/hooks/organize-undo'

afterEach(() => {
  vi.clearAllMocks()
  harness.mutates.length = 0
})

describe('selectAllQuery', () => {
  it('falls back to the all:true sentinel for a blank filter', () => {
    expect(selectAllQuery('')).toBe('all:true')
    expect(selectAllQuery('   ')).toBe('all:true')
  })

  it('keeps a real filter untouched', () => {
    expect(selectAllQuery('trash:true')).toBe('trash:true')
    expect(selectAllQuery(' bucket_id:12 ')).toBe('bucket_id:12')
  })
})

describe('useDelete select-all', () => {
  const done = vi.fn()

  it('sends all:true when select-all has no filter (calls page scenario)', () => {
    const d = useDelete('delete-calls', done)
    d.deleteItems([], true, 42, '')
    expect(d.confirmingDelete.value).toBe(true)
    d.doDeleteItems()
    expect(harness.mutates[0]).toHaveBeenCalledWith({ query: 'all:true' })
  })

  it('keeps the active filter when select-all is filtered', () => {
    const d = useDelete('delete-notes', done)
    d.deleteItems([], true, 7, 'trash:true')
    d.doDeleteItems()
    expect(harness.mutates[0]).toHaveBeenCalledWith({ query: 'trash:true' })
  })

  it('sends ids: for a plain selection and nothing for an empty one', () => {
    const d = useDelete('delete-calls', done)
    d.deleteItems(['1', '2'], false, 42, '')
    d.doDeleteItems()
    expect(harness.mutates[0]).toHaveBeenCalledWith({ query: 'ids:1,2' })

    harness.mutates.length = 0
    const empty = useDelete('delete-calls', done)
    empty.deleteItems([], false, 42, '')
    expect(empty.confirmingDelete.value).toBe(false)
    // initMutation registers a mock on construction; it must never be invoked.
    harness.mutates.forEach((m) => expect(m).not.toHaveBeenCalled())
  })
})

describe('media hooks select-all', () => {
  it('useDeleteItems sends all:true for an unfiltered select-all', () => {
    const { deleteItems, doDeleteItems } = useDeleteItems()
    deleteItems('IMAGE', [], true, 100, '')
    doDeleteItems()
    expect(harness.mutates[0]).toHaveBeenCalledWith({ type: 'IMAGE', query: 'all:true' })
  })

  it('useMoveItems resolves all:true for an unfiltered select-all', () => {
    const { moveItems } = useMoveItems()
    expect(moveItems('IMAGE', [], true, '')).toBe('all:true')
    expect(moveItems('IMAGE', ['a'], false, '')).toBe('ids:a')
    expect(moveItems('IMAGE', [], true, 'bucket_id:9')).toBe('bucket_id:9')
  })
})

describe('organize-undo record guard', () => {
  it('never stacks a blank query', () => {
    const { count, record, undoLast } = useOrganizeUndo()
    record({ kind: 'trash', type: 'IMAGE' as DataType, query: '' })
    record({ kind: 'trash', type: 'IMAGE' as DataType, query: '   ' })
    expect(count.value).toBe(0)
    expect(undoLast()).toBeNull()
  })

  it('still stacks targeted queries', () => {
    const { count, record } = useOrganizeUndo()
    record({ kind: 'trash', type: 'IMAGE' as DataType, query: 'ids:5' })
    expect(count.value).toBe(1)
  })
})
