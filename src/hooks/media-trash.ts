import { initMutation, trashMediaItemsGQL, restoreMediaItemsGQL } from '@/lib/api/mutation'
import { DataType, FEATURE } from '@/lib/data'
import emitter from '@/plugins/eventbus'

import { reactive, computed, ref, type Ref } from 'vue'
import { hasFeature } from '@/lib/feature'
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

export function useFileTrashState(
  current: (() => ISource | undefined) | Ref<ISource | undefined>, 
  osVersion: (() => number) | Ref<number> | number
) {
  const isTrashed = computed(() => {
    const currentValue = typeof current === 'function' ? current() : current.value
    return currentValue?.path?.includes('.trashed-') === true
  })

  const canTrash = computed(() => {
    const currentValue = typeof current === 'function' ? current() : current.value
    const mediaTypes = [DataType.VIDEO, DataType.AUDIO, DataType.IMAGE]
    const type = currentValue?.type
    
    let version: number
    if (typeof osVersion === 'number') {
      version = osVersion
    } else if (typeof osVersion === 'function') {
      version = osVersion()
    } else {
      version = osVersion.value
    }
    
    return type && mediaTypes.includes(type as DataType) && hasFeature(FEATURE.MEDIA_TRASH, version)
  })

  return {
    isTrashed,
    canTrash
  }
}
