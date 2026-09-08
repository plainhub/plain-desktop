import type { IData } from '@/lib/interfaces'
import { computed, ref, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import { initMutation } from '@/lib/api/mutation'

export const useSelectable = (items: Ref<IData[]>) => {
  const allChecked = ref(false)
  const realAllChecked = ref(false)
  const total = ref(0)
  const selectedIds = ref<string[]>([])
  // shift select
  const lastCheckedIndex = ref<number | null>(null)
  const shiftEffectingIds = ref<string[]>([])
  const shouldSelect = ref(false)

  const updateAllCheckState = (checked: boolean) => {
    if (checked) {
      allChecked.value = items.value.every((it) => selectedIds.value.includes(it.id))
    } else {
      allChecked.value = false
      realAllChecked.value = false
    }
  }

  const getShiftEffectingIds = (index: number) => {
    if (lastCheckedIndex.value === null) return []
    const start = Math.min(lastCheckedIndex.value, index)
    const end = Math.max(lastCheckedIndex.value, index)
    const lastIndexId = items.value[lastCheckedIndex.value].id
    return items.value
      .slice(start, end + 1)
      .map((it) => it.id)
      .filter((it) => it !== lastIndexId)
  }

  const toggleShiftSelection = (item: IData, index: number) => {
    // 没有锚点或点了同一项，回退到普通 toggle
    if (lastCheckedIndex.value === null || lastCheckedIndex.value === index) {
      checkItem(!selectedIds.value.includes(item.id), item, index)
      return
    }
    // 优先用 hover 预览的 ids；没有预览就实时计算区间，让"裸 shift+click"也能范围选择
    const ids = shiftEffectingIds.value.length > 0
      ? shiftEffectingIds.value
      : getShiftEffectingIds(index)
    if (ids.length > 0) {
      if (shouldSelect.value) {
        for (const id of ids) {
          if (!selectedIds.value.includes(id)) {
            selectedIds.value.push(id)
          }
        }
      } else {
        selectedIds.value = selectedIds.value.filter((it) => !ids.includes(it))
      }
    } else {
      checkItem(!selectedIds.value.includes(item.id), item, index)
      return
    }
    shiftEffectingIds.value = []
    lastCheckedIndex.value = index
    shouldSelect.value = selectedIds.value.includes(item.id)
    updateAllCheckState(shouldSelect.value)
  }

  const checkItem = (checked: boolean, item: IData, index: number) => {
    if (checked) {
      selectedIds.value.push(item.id)
    } else {
      selectedIds.value = selectedIds.value.filter((id) => id !== item.id)
    }
    lastCheckedIndex.value = index
    shouldSelect.value = checked
    updateAllCheckState(checked)
  }

  const selectAll = () => {
    allChecked.value = true
    selectedIds.value = items.value.map((it) => it.id)
  }

  const groupSelectionState = (groupItems: IData[]) => {
    const count = groupItems.reduce((acc, it) => acc + (selectedIds.value.includes(it.id) ? 1 : 0), 0)
    return {
      checked: groupItems.length > 0 && count === groupItems.length,
      indeterminate: count > 0 && count < groupItems.length,
    }
  }

  const setGroupChecked = (groupItems: IData[], checked: boolean) => {
    if (checked) {
      for (const it of groupItems) {
        if (!selectedIds.value.includes(it.id)) {
          selectedIds.value.push(it.id)
        }
      }
    } else {
      const ids = new Set(groupItems.map((it) => it.id))
      selectedIds.value = selectedIds.value.filter((id) => !ids.has(id))
    }
    updateAllCheckState(checked)
  }

  const toggleGroupChecked = (event: Event, groupItems: IData[]) => {
    setGroupChecked(groupItems, (event.target as HTMLInputElement).checked)
  }

  return {
    realAllChecked,
    allChecked,
    toggleAllChecked: (event: Event) => {
      if ((event.target as HTMLInputElement).checked) {
        selectAll()
      } else {
        allChecked.value = false
        realAllChecked.value = false
        selectedIds.value = []
      }
    },
    selectAll,
    groupSelectionState,
    setGroupChecked,
    toggleGroupChecked,
    allCheckedAlertVisible: computed<boolean>(() => {
      return allChecked.value && !realAllChecked.value && selectedIds.value.length < total.value
    }),
    selectRealAll: () => {
      realAllChecked.value = true
    },
    clearSelection: () => {
      allChecked.value = false
      realAllChecked.value = false
      selectedIds.value = []
      lastCheckedIndex.value = null
      shouldSelect.value = false
      shiftEffectingIds.value = []
    },
    selectedIds,
    total,
    checked: computed<boolean>(() => {
      return selectedIds.value.length > 0
    }),
    // shift select
    shouldSelect,
    shiftEffectingIds,
    toggleSelect: (event: MouseEvent, item: IData, index: number) => {
      if (event.shiftKey) {
        toggleShiftSelection(item, index)
      } else {
        checkItem(!selectedIds.value.includes(item.id), item, index)
      }
    },
    handleItemClick(event: MouseEvent, item: IData, index: number, view: (i: number) => void = () => {}) {
      if ((event.target as Element)?.nodeName === 'V-CHECKBOX') {
        return
      }

      const selection = window.getSelection()
      if (selection && selection.toString()) {
        const currentTarget = event.currentTarget as Element
        if (currentTarget && selection.rangeCount > 0) {
          const range = selection.getRangeAt(0)
          if (currentTarget.contains(range.commonAncestorContainer)) {
            return
          }
        }
      }

      if (selectedIds.value.length === 0) {
        view(index)
        return
      }
      if (event.shiftKey) {
        toggleShiftSelection(item, index)
      } else {
        checkItem(!selectedIds.value.includes(item.id), item, index)
      }
    },
    handleMouseOver(event: MouseEvent, index: number) {
      if (event.shiftKey) {
        if (lastCheckedIndex.value !== null && lastCheckedIndex.value !== index) {
          shiftEffectingIds.value = getShiftEffectingIds(index)
        }
      } else {
        shiftEffectingIds.value = []
      }
    },
  }
}

export const useDelete = (gql: string, done: () => void) => {
  const { t } = useI18n()
  const confirmingDelete = ref(false)
  const deleteCount = ref(0)
  const deleteQuery = ref('')

  const { mutate, loading: deleteLoading, onDone } = initMutation({ document: gql })

  onDone(() => {
    confirmingDelete.value = false
    done()
  })

  return {
    confirmingDelete,
    deleteCount,
    deleteLoading,
    deleteItems: (selectedIds: string[], realAllChecked: boolean, total: number, query: string) => {
      let q = query
      if (!realAllChecked) {
        if (selectedIds.length === 0) {
          toast(t('select_first'), 'error')
          return
        }
        q = `ids:${selectedIds.join(',')}`
      }
      deleteCount.value = realAllChecked ? total : selectedIds.length
      deleteQuery.value = q
      confirmingDelete.value = true
    },
    doDeleteItems: () => {
      mutate({ query: deleteQuery.value })
    },
    cancelDeleteItems: () => {
      confirmingDelete.value = false
    },
  }
}
