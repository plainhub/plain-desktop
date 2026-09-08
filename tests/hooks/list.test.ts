import { afterEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import type { IData } from '@/lib/interfaces'

vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/lib/api/mutation', () => ({
  initMutation: () => ({ mutate: vi.fn(), loading: ref(false), onDone: vi.fn() }),
}))

import { useSelectable } from '@/hooks/list'

const items = [{ id: 'a' }, { id: 'b' }, { id: 'c' }] as IData[]

const changeEvent = (checked: boolean) => ({ target: { checked } }) as unknown as Event

afterEach(() => {
  vi.clearAllMocks()
})

describe('useSelectable group selection', () => {
  it('reports checked / indeterminate state for a date group', () => {
    const sel = useSelectable(ref(items))
    sel.toggleSelect({ shiftKey: false } as MouseEvent, items[0], 0)

    expect(sel.groupSelectionState([items[0]]).checked).toBe(true)
    expect(sel.groupSelectionState([items[0]]).indeterminate).toBe(false)

    const partial = sel.groupSelectionState([items[0], items[1]])
    expect(partial.checked).toBe(false)
    expect(partial.indeterminate).toBe(true)

    expect(sel.groupSelectionState([])).toEqual({ checked: false, indeterminate: false })
  })

  it('selects and deselects every item of a group', () => {
    const sel = useSelectable(ref(items))
    const group = [items[0], items[2]]

    sel.toggleGroupChecked(changeEvent(true), group)
    expect(sel.selectedIds.value).toEqual(['a', 'c'])

    sel.toggleGroupChecked(changeEvent(true), group)
    expect(sel.selectedIds.value).toEqual(['a', 'c'])

    sel.toggleGroupChecked(changeEvent(false), group)
    expect(sel.selectedIds.value).toEqual([])
  })

  it('flags all-checked alert when a group completes the loaded items', () => {
    const sel = useSelectable(ref(items))
    sel.total.value = 5

    sel.toggleGroupChecked(changeEvent(true), items)
    expect(sel.allChecked.value).toBe(true)
    expect(sel.allCheckedAlertVisible.value).toBe(true)

    sel.toggleGroupChecked(changeEvent(false), [items[0]])
    expect(sel.allChecked.value).toBe(false)
    expect(sel.allCheckedAlertVisible.value).toBe(false)
    expect(sel.selectedIds.value).toEqual(['b', 'c'])
  })

  it('toggles a group via setGroupChecked (date text click path)', () => {
    const sel = useSelectable(ref(items))
    sel.setGroupChecked([items[0], items[1]], true)
    expect(sel.selectedIds.value).toEqual(['a', 'b'])
    expect(sel.groupSelectionState([items[0], items[1]]).checked).toBe(true)

    sel.setGroupChecked([items[1]], false)
    expect(sel.selectedIds.value).toEqual(['a'])
    const state = sel.groupSelectionState([items[0], items[1]])
    expect(state.checked).toBe(false)
    expect(state.indeterminate).toBe(true)
  })

  it('resets real-all when a group is deselected', () => {
    const sel = useSelectable(ref(items))
    sel.selectAll()
    sel.realAllChecked.value = true

    sel.toggleGroupChecked(changeEvent(false), [items[0]])
    expect(sel.realAllChecked.value).toBe(false)
    expect(sel.selectedIds.value).toEqual(['b', 'c'])
  })
})
