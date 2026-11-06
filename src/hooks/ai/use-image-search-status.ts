import { ref, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { imageSearchStatusGQL, initQuery } from '@/lib/api/query'
import toast from '@/components/toaster'
import type { IImageSearchStatus } from '@/lib/interfaces'

export function useImageSearchStatus() {
  const { t } = useI18n()
  const status = ref<IImageSearchStatus | null>(null)

  initQuery({
    handle: (data: { imageSearchStatus: IImageSearchStatus }, error: string) => {
      if (error) toast(t(error), 'error')
      else if (data) status.value = { ...data.imageSearchStatus }
    },
    document: imageSearchStatusGQL,
    variables: null,
  })

  function onStatusUpdated(data: IImageSearchStatus) {
    if (data) status.value = { ...data }
  }

  const stop = graphqlSubscriptions.subscribe('imageModelsUpdated', onStatusUpdated)
  onUnmounted(stop)

  return { status }
}
