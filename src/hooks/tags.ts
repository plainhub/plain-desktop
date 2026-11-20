import { AccessErrorCode } from '@/lib/api/access-error-code'
import type { ITag } from '@/lib/interfaces'
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import { initLazyQuery, tagsGQL } from '@/lib/api/query'

export const useTags = (type: string, onLoad: () => void = () => {}) => {
  const tags = ref<ITag[]>([])
  const { t } = useI18n()

  const { loading, fetch } = initLazyQuery({
    handle: async (data: any, error: string) => {
      if (error === AccessErrorCode.NoPermission) {
        tags.value = []
      } else if (error) {
        toast(t(error), 'error')
      } else {
        if (data) {
          tags.value = data.tags
          onLoad()
        }
      }
    },
    document: tagsGQL,
    variables: {
      type: type,
    },
  })
  return {
    tags,
    loading,
    fetch,
  }
}
