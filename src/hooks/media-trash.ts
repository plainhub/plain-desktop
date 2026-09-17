import { initMutation, trashMediaItemsGQL, restoreMediaItemsGQL } from '@/lib/api/mutation'
import { DataType } from '@/lib/data'
import emitter from '@/plugins/eventbus'

import { reactive, computed, type Ref } from 'vue'
import { hasMediaTrash } from '@/lib/feature'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import type { ISource } from '@/components/lightbox/types'

export const useMediaTrash = () => {
  const { mutate, onDone: onTrashed } = initMutation({
    document: trashMediaItemsGQL,
  })

  const loading = reactive(new Map())

  onTrashed((r: any) => {
    const { type, query } = r.data.trashMediaItems
    loading.delete(query)
    emitter.emit('refetch_tags', type)
    emitter.emit('media_items_actioned', { type, action: 'trash', query })
  })

  return {
    trashLoading(query: string) {
      return loading.get(query) ?? false
    },
    trash(type: DataType, query: string) {
      loading.set(query, true)
      mutate({ query, type })
    },
  }
}

export const useMediaRestore = () => {
  const { mutate, onDone: onRestored } = initMutation({
    document: restoreMediaItemsGQL,
  })

  const loading = reactive(new Map())

  onRestored((r: any) => {
    const { type, query } = r.data.restoreMediaItems
    loading.delete(query)
    emitter.emit('refetch_tags', type)
    emitter.emit('media_items_actioned', { type, action: 'restore', query })
  })

  return {
    restoreLoading(query: string) {
      return loading.get(query) ?? false
    },
    restore(type: DataType, query: string) {
      loading.set(query, true)
      mutate({ query, type })
    },
  }
}

export function useFileTrashState(current: (() => ISource | undefined) | Ref<ISource | undefined>) {
  const { app } = storeToRefs(useTempStore())
  const isTrashed = computed(() => {
    const currentValue = typeof current === 'function' ? current() : current.value
    const path = currentValue?.path ?? ''
    // Phones trash via MediaStore (files renamed with a `.trashed-` prefix);
    // a NAS moves files into its `.nas-trash` tree. Either marker means trash.
    return path.includes('.trashed-') || path.includes('/.nas-trash/')
  })

  const canTrash = computed(() => {
    const currentValue = typeof current === 'function' ? current() : current.value
    const mediaTypes = [DataType.VIDEO, DataType.AUDIO, DataType.IMAGE]
    return !!currentValue?.type && mediaTypes.includes(currentValue.type as DataType) && hasMediaTrash(app.value)
  })

  return {
    isTrashed,
    canTrash
  }
}
