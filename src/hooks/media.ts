import { AccessErrorCode } from '@/lib/api/access-error-code'
import { showActionError } from '@/hooks/show-action-error'
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
import { selectAllQuery } from '@/hooks/list'
import { replacePath } from '@/plugins/router'
import type { IAudio, IBucket, IImageItem, ITag, IVideoItem } from '@/lib/interfaces'
import { ref } from 'vue'
import { bucketsTagsGQL, initLazyQuery } from '@/lib/api/query'

export function useDeleteMediaItem() {
  const { mutate } = initMutation({ document: deleteMediaItemsGQL }, showActionError)
  return async (type: string, item: IImageItem | IVideoItem | IAudio) => {
    const query = `ids:${item.id}`
    const result = await mutate({ type, query })
    if (result) emitter.emit('media_items_actioned', { type, action: 'DELETE', id: item.id, query })
    return result
  }
}

export const useMoveItems = () => {
  const { t } = useI18n()
  const moveType = ref('')
  const moveQuery = ref('')

  const { mutate: doMove, loading: moveLoading, onDone: onMoveDone } = initMutation({ document: moveMediaItemsGQL }, showActionError)
  onMoveDone(() => {
    const type = moveType.value
    const q = moveQuery.value
    emitter.emit('media_items_actioned', { type, action: 'move', query: q })
  })

  return {
    moveLoading,
    moveItems: (type: string, ids: string[], realAllChecked: boolean, query: string) => {
      let q: string
      if (!realAllChecked) {
        if (ids.length === 0) {
          toast(t('select_first'), 'error')
          return
        }
        q = `ids:${ids.join(',')}`
      } else {
        q = selectAllQuery(query)
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
      if (error === AccessErrorCode.NoPermission) {
        tags.value = []
        buckets.value = []
      } else if (error) {
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
      tagType: type,
    },
  })

  return {
    tags,
    buckets,
    fetch,
  }
}
