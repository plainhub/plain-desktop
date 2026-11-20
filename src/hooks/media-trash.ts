import { showActionError } from '@/hooks/show-action-error'
import { initMutation, trashMediaItemsGQL, restoreMediaItemsGQL } from '@/lib/api/mutation'
import { DataType } from '@/lib/data'
import emitter from '@/plugins/eventbus'

import { reactive, computed, type Ref } from 'vue'
import { hasMediaTrash, hasMediaDelete, hasMediaMove } from '@/lib/feature'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { isTrashedMediaPath } from '@/lib/media/trash-path'
import type { ISource } from '@/components/lightbox/types'

function useMediaTrashAction(action: 'trash' | 'restore') {
  const { mutate } = initMutation({ document: action === 'trash' ? trashMediaItemsGQL : restoreMediaItemsGQL }, showActionError)
  const pending = reactive(new Set<string>())

  async function apply(type: DataType, query: string) {
    const key = `${type}:${query}`
    if (pending.has(key)) return false
    pending.add(key)
    try {
      const result = await mutate({ query, type })
      const count = (action === 'trash' ? result?.data?.trashMediaItems : result?.data?.restoreMediaItems)?.affectedCount
      if (!count) return false
      const partial = query.startsWith('ids:') && count !== query.slice(4).split(',').length
      emitter.emit('refetch_tags', type)
      emitter.emit('media_items_actioned', { type, action, query: partial ? '' : query })
      return true
    } finally {
      pending.delete(key)
    }
  }

  return {
    apply,
    isPending: (query: string) => [...pending].some((key) => key.slice(key.indexOf(':') + 1) === query),
  }
}

export function useMediaTrash() {
  const { apply, isPending } = useMediaTrashAction('trash')
  return { trash: apply, trashLoading: isPending }
}

export function useMediaRestore() {
  const { apply, isPending } = useMediaTrashAction('restore')
  return { restore: apply, restoreLoading: isPending }
}

export function useFileTrashState(current: (() => ISource | undefined) | Ref<ISource | undefined>) {
  const { app } = storeToRefs(useTempStore())
  const source = computed(() => typeof current === 'function' ? current() : current.value)
  const type = computed(() => source.value?.type as DataType | undefined)
  const isTrashed = computed(() => isTrashedMediaPath(source.value?.path))
  const isMedia = computed(() => !!type.value && [DataType.VIDEO, DataType.AUDIO, DataType.IMAGE, DataType.DOC].includes(type.value))
  const canTrash = computed(() => isMedia.value && hasMediaTrash(app.value, type.value))
  const canDelete = computed(() => !isMedia.value || hasMediaDelete(app.value))
  const canMove = computed(() => !isMedia.value || hasMediaMove(app.value))

  return { isTrashed, canTrash, canDelete, canMove }
}
