import { expect, it, vi } from 'vitest'
import { effectScope, ref } from 'vue'
import type { IData } from '@/lib/interfaces'
import { useDelete, useSelectable } from '@/hooks/list'

const mutate = vi.hoisted(() => vi.fn().mockResolvedValue({ data: {} }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/hooks/show-action-error', () => ({ showActionError: vi.fn() }))
vi.mock('@/lib/api/mutation', () => ({ initMutation: () => ({ mutate, loading: ref(false), onDone: vi.fn() }) }))

const row = (id: string) => ({ id }) as IData
const [a, b, c, d] = ['a', 'b', 'c', 'd'].map(row)
const click = { shiftKey: false } as MouseEvent
const shift = { shiftKey: true } as MouseEvent

function setup() {
  const scope = effectScope()
  mutate.mockClear()
  return scope.run(() => {
    const items = ref([a, b, c])
    const selection = useSelectable(items)
    const query = ref('tag:one')
    const deletion = useDelete('delete', selection, query, vi.fn())
    selection.total.value = 10
    return { scope, items, selection, query, deletion }
  })!
}

it('drops a missing Shift anchor and never submits a superseded delete confirmation', async () => {
  const { scope, items, selection, deletion } = setup()
  try {
    selection.toggleSelect(click, c, 2)
    deletion.deleteItems()
    expect(deletion.confirmingDelete.value).toBe(true)

    items.value = [a, b]
    expect(selection.selectedIds.value).toEqual([])
    expect(selection.shiftEffectingIds.value).toEqual([])
    expect(deletion.confirmingDelete.value).toBe(false)
    await deletion.doDeleteItems()
    expect(mutate).not.toHaveBeenCalled()

    selection.toggleSelect(shift, b, 1)
    expect(selection.selectedIds.value).toEqual(['b'])
    deletion.deleteItems()
    await deletion.doDeleteItems()
    expect(mutate).toHaveBeenCalledExactlyOnceWith({ query: 'ids:b' })
  } finally { scope.stop() }
})

it('keeps a surviving anchor by ID after reorder and recalculates the Shift range', () => {
  const scope = effectScope()
  try {
    scope.run(() => {
      const items = ref([a, b, c])
      const selection = useSelectable(items)
      selection.toggleSelect(click, c, 2)
      selection.handleMouseOver(shift, 0)
      items.value = [c, b, a]
      expect(selection.shiftEffectingIds.value).toEqual([])
      selection.toggleSelect(shift, b, 1)
      expect(selection.selectedIds.value).toEqual(['c', 'b'])
    })
  } finally { scope.stop() }
})

it('keeps an explicit confirmation through harmless refresh and invalidates global selection when membership changes', async () => {
  const { scope, items, selection, deletion } = setup()
  try {
    selection.toggleSelect(click, a, 0)
    deletion.deleteItems()
    items.value = [c, a, b]
    expect(deletion.confirmingDelete.value).toBe(true)
    await deletion.doDeleteItems()
    expect(mutate).toHaveBeenCalledExactlyOnceWith({ query: 'ids:a' })

    selection.selectAll()
    selection.selectRealAll()
    deletion.deleteItems()
    expect(deletion.deleteCount.value).toBe(10)
    items.value = [a, b, d]
    expect(selection.selectedIds.value).toEqual(['a', 'b'])
    expect(selection.realAllChecked.value).toBe(false)
    expect(deletion.confirmingDelete.value).toBe(false)
    await deletion.doDeleteItems()
    expect(mutate).toHaveBeenCalledTimes(1)
  } finally { scope.stop() }
})

it('requires a new query-wide confirmation when the filter or result count changes', async () => {
  const { scope, selection, deletion, query } = setup()
  try {
    selection.selectAll()
    selection.selectRealAll()
    deletion.deleteItems()

    query.value = 'tag:two'
    expect(deletion.confirmingDelete.value).toBe(false)
    await deletion.doDeleteItems()
    expect(mutate).not.toHaveBeenCalled()

    deletion.deleteItems()
    expect(deletion.deleteCount.value).toBe(10)
    selection.total.value = 9
    expect(deletion.confirmingDelete.value).toBe(false)

    deletion.deleteItems()
    await deletion.doDeleteItems()
    expect(mutate).toHaveBeenCalledExactlyOnceWith({ query: 'tag:two' })
  } finally { scope.stop() }
})
