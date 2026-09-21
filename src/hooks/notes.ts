import { initMutation, restoreNotesGQL, trashNotesGQL } from '@/lib/api/mutation'
import { DataType } from '@/lib/data'
import emitter from '@/plugins/eventbus'

import { reactive } from 'vue'

export const useNotesTrash = (clearSelection: () => void, fetch: () => void) => {
  const { mutate, onDone: onTrashed } = initMutation({
    document: trashNotesGQL,
  })

  const loading = reactive(new Map())

  onTrashed(() => {
    loading.clear()
    clearSelection()
    fetch()
    emitter.emit('refetch_tags', DataType.NOTE)
    emitter.emit('notes_actioned', { action: 'trash' })
  })

  return {
    trashLoading(query: string) {
      return loading.get(query) ?? false
    },
    trash(query: string) {
      loading.set(query, true)
      mutate({ query })
    },
  }
}

export const useNotesRestore = (clearSelection: () => void, fetch: () => void) => {
  const { mutate, onDone: onRestored } = initMutation({
    document: restoreNotesGQL,
  })

  const loading = reactive(new Map())

  onRestored(() => {
    loading.clear()
    clearSelection()
    fetch()
    emitter.emit('refetch_tags', DataType.NOTE)
    emitter.emit('notes_actioned', { action: 'restore' })
  })

  return {
    restoreLoading(query: string) {
      return loading.get(query) ?? false
    },
    restore(query: string) {
      loading.set(query, true)
      mutate({ query })
    },
  }
}
