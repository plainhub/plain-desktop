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

describe('useSelectable selection state', () => {
  it('toggles an item and keeps selectedIdSet in sync', () => {
    const sel = useSelectable(ref(items))

    sel.toggleSelect({ shiftKey: false } as MouseEvent, items[0], 0)
    expect(sel.selectedIds.value).toEqual(['a'])
    expect(sel.selectedIdSet.value.has('a')).toBe(true)
    expect(sel.checked.value).toBe(true)

    sel.toggleSelect({ shiftKey: false } as MouseEvent, items[0], 0)
    expect(sel.selectedIds.value).toEqual([])
    expect(sel.selectedIdSet.value.has('a')).toBe(false)
    expect(sel.checked.value).toBe(false)
  })

  it('flags all-checked once every loaded item is individually selected', () => {
    const sel = useSelectable(ref(items))
    sel.toggleSelect({ shiftKey: false } as MouseEvent, items[0], 0)
    expect(sel.allChecked.value).toBe(false)

    sel.toggleSelect({ shiftKey: false } as MouseEvent, items[1], 1)
    sel.toggleSelect({ shiftKey: false } as MouseEvent, items[2], 2)
    expect(sel.allChecked.value).toBe(true)
    expect(sel.selectedIdSet.value.size).toBe(3)
  })

  it('shift-click selects the anchor..index range', () => {
    const sel = useSelectable(ref(items))
    sel.toggleSelect({ shiftKey: false } as MouseEvent, items[0], 0)

    sel.toggleSelect({ shiftKey: true } as MouseEvent, items[2], 2)
    expect(sel.selectedIds.value).toEqual(['a', 'b', 'c'])
    expect(sel.allChecked.value).toBe(true)
    expect(sel.shouldSelect.value).toBe(true)
  })

  it('shift-click deselects the range when the anchor was deselected', () => {
    const sel = useSelectable(ref(items))
    sel.selectAll()
    sel.toggleSelect({ shiftKey: false } as MouseEvent, items[0], 0)
    expect(sel.selectedIds.value).toEqual(['b', 'c'])

    sel.toggleSelect({ shiftKey: true } as MouseEvent, items[2], 2)
    expect(sel.selectedIds.value).toEqual([])
  })

  it('shift-click uses the hover preview range when present', () => {
    const sel = useSelectable(ref(items))
    sel.toggleSelect({ shiftKey: false } as MouseEvent, items[0], 0)

    sel.handleMouseOver({ shiftKey: true } as MouseEvent, 2)
    expect(sel.shiftEffectingIds.value).toEqual(['b', 'c'])
    expect(sel.shiftEffectingIdSet.value.has('b')).toBe(true)
    expect(sel.shiftEffectingIdSet.value.has('a')).toBe(false)

    sel.toggleSelect({ shiftKey: true } as MouseEvent, items[2], 2)
    expect(sel.selectedIds.value).toEqual(['a', 'b', 'c'])
    expect(sel.shiftEffectingIds.value).toEqual([])
  })

  it('clears hover preview when shift is released', () => {
    const sel = useSelectable(ref(items))
    sel.toggleSelect({ shiftKey: false } as MouseEvent, items[0], 0)

    sel.handleMouseOver({ shiftKey: true } as MouseEvent, 2)
    expect(sel.shiftEffectingIdSet.value.size).toBe(2)

    sel.handleMouseOver({ shiftKey: false } as MouseEvent, 2)
    expect(sel.shiftEffectingIdSet.value.size).toBe(0)
  })

  it('clearSelection resets ids, sets and shift anchors', () => {
    const sel = useSelectable(ref(items))
    sel.toggleSelect({ shiftKey: false } as MouseEvent, items[0], 0)
    sel.handleMouseOver({ shiftKey: true } as MouseEvent, 2)

    sel.clearSelection()
    expect(sel.selectedIds.value).toEqual([])
    expect(sel.selectedIdSet.value.size).toBe(0)
    expect(sel.shiftEffectingIdSet.value.size).toBe(0)
    expect(sel.checked.value).toBe(false)
    expect(sel.shouldSelect.value).toBe(false)
  })

  it('handleItemClick opens the view when nothing is selected, toggles otherwise', () => {
    const sel = useSelectable(ref(items))
    const view = vi.fn()

    sel.handleItemClick({ shiftKey: false } as MouseEvent, items[1], 1, view)
    expect(view).toHaveBeenCalledWith(1)
    expect(sel.selectedIds.value).toEqual([])

    sel.toggleSelect({ shiftKey: false } as MouseEvent, items[0], 0)
    sel.handleItemClick({ shiftKey: false } as MouseEvent, items[1], 1, view)
    expect(view).toHaveBeenCalledTimes(1)
    expect(sel.selectedIds.value).toEqual(['a', 'b'])
  })
})
