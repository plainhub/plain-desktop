import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import { promptModal } from '@/components/modal'
import DirectoryPickerModal from '@/components/DirectoryPickerModal.vue'
import { deleteMediaItemsGQL, initMutation, moveMediaItemsGQL } from '@/lib/api/mutation'
import emitter from '@/plugins/eventbus'
import type { DataType } from '@/lib/data'
import { encodeBase64 } from '@/lib/strutil'
import type { MainState } from '@/stores/main'
import { buildQuery } from '@/lib/search'
import { replacePath } from '@/plugins/router'
import type { IAudio, IBucket, IImageItem, ITag, IVideoItem } from '@/lib/interfaces'
import { ref } from 'vue'
import { bucketsTagsGQL, initLazyQuery } from '@/lib/api/query'

export const useDeleteItems = () => {
  const { t } = useI18n()
  const confirmingDelete = ref(false)
  const deleteCount = ref(0)
  const deleteQuery = ref('')
  const deleteType = ref('')

  const { mutate: doBulkDelete, loading: deleteLoading, onDone: onBulkDeleteDone } = initMutation({ document: deleteMediaItemsGQL })
  onBulkDeleteDone(() => {
    const type = deleteType.value
    const q = deleteQuery.value
    confirmingDelete.value = false
    emitter.emit('media_items_actioned', { type, action: 'delete', query: q })
  })

  const { mutate: doDeleteItem, onDone: onDeleteItemDone } = initMutation({ document: deleteMediaItemsGQL })
  let pendingDeleteCallback: (() => void) | null = null
  onDeleteItemDone(() => { pendingDeleteCallback?.(); pendingDeleteCallback = null })

  return {
    confirmingDelete,
    deleteCount,
    deleteLoading,
    deleteItems: (type: string, ids: string[], realAllChecked: boolean, total: number, query: string) => {
      let q = query
      if (!realAllChecked) {
        if (ids.length === 0) {
          toast(t('select_first'), 'error')
          return
        }
        q = `ids:${ids.join(',')}`
      }
      deleteCount.value = realAllChecked ? total : ids.length
      deleteQuery.value = q
      deleteType.value = type
      confirmingDelete.value = true
    },
    doDeleteItems: () => {
      doBulkDelete({ type: deleteType.value, query: deleteQuery.value })
    },
    cancelDeleteItems: () => {
      confirmingDelete.value = false
    },

    deleteItem: (type: string, item: IImageItem | IVideoItem | IAudio) => {
      pendingDeleteCallback = () => {
        emitter.emit('media_items_actioned', { type, action: 'delete', id: item.id, query: `ids:${item.id}` })
      }
      doDeleteItem({ type, query: `ids:${item.id}` })
    },
  }
}


export const useMoveItems = () => {
  const { t } = useI18n()
  const moveType = ref('')
  const moveQuery = ref('')

  const { mutate: doMove, loading: moveLoading, onDone: onMoveDone } = initMutation({ document: moveMediaItemsGQL })
  onMoveDone(() => {
    const type = moveType.value
    const q = moveQuery.value
    emitter.emit('media_items_actioned', { type, action: 'move', query: q })
  })

  return {
    moveLoading,
    moveItems: (type: string, ids: string[], realAllChecked: boolean, query: string) => {
      let q = query
      if (!realAllChecked) {
        if (ids.length === 0) {
          toast(t('select_first'), 'error')
          return
        }
        q = `ids:${ids.join(',')}`
      }
      moveType.value = type
      moveQuery.value = q
      return q
    },
    doMoveItems: (destDir: string) => {
      doMove({ type: moveType.value, query: moveQuery.value, destDir })
    },
  }
}

export const useMoveToFolder = (modalId: string) => {
  const { t } = useI18n()
  const { moveLoading, moveItems, doMoveItems } = useMoveItems()
  const onMoveClick = async (type: string, ids: string[], realAllChecked: boolean, query: string) => {
    const q = moveItems(type, ids, realAllChecked, query)
    if (q === undefined) return
    const destDir = await promptModal<string>(DirectoryPickerModal, {
      title: t('move_to_folder'),
      modalId,
    })
    if (typeof destDir !== 'string' || !destDir.trim()) return
    doMoveItems(destDir.trim())
  }
  return { moveLoading, onMoveClick }
}


export const useBuckets = (type: DataType) => {
  const path = ({
    AUDIO: 'audios',
    IMAGE: 'images',
    VIDEO: 'videos',
    DOC: 'docs',
  } as Record<string, string>)[type as string]
  return {
    view(mainStore: MainState, id: string) {
      const q = buildQuery([
        {
          name: 'bucket_id',
          op: '',
          value: id,
        },
      ])
      replacePath(mainStore, `/${path}?q=${encodeBase64(q)}`)
    },
  }
}

export const useBucketsTags = (type: DataType) => {
  const tags = ref<ITag[]>([])
  const buckets = ref<IBucket[]>([])
  const { t } = useI18n()
  const { fetch } = initLazyQuery({
    handle: async (data: { tags: ITag[]; mediaBuckets: IBucket[] }, error: string) => {
      if (error) {
        toast(t(error), 'error')
      } else {
        if (data) {
          tags.value = data.tags
          buckets.value = data.mediaBuckets
        }
      }
    },
    document: bucketsTagsGQL,
    variables: {
      type,
    },
  })

  return {
    tags,
    buckets,
    fetch,
  }
}
