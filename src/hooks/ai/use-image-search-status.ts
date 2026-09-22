import { ref, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import emitter from '@/plugins/eventbus'
import { imageSearchStatusGQL, initQuery } from '@/lib/api/query'
import { Capability } from '@/lib/data'
import { hasFeature } from '@/lib/feature'
import toast from '@/components/toaster'
import { useTempStore } from '@/stores/temp'
import type { IImageSearchStatus } from '@/lib/interfaces'

export function useImageSearchStatus() {
  const { t } = useI18n()
  const { app } = storeToRefs(useTempStore())
  const status = ref<IImageSearchStatus | null>(null)

  initQuery({
    handle: (data: { imageSearchStatus: IImageSearchStatus }, error: string) => {
      if (error) toast(t(error), 'error')
      else if (data) status.value = { ...data.imageSearchStatus }
    },
    document: imageSearchStatusGQL,
    variables: null,
    // AI image search is phone-only; on a NAS (or before the device type is
    // known) the query is never sent — the entry is hidden anyway.
    enabled: () => hasFeature(Capability.IMAGE_SEARCH, app.value?.capabilities),
  })

  function onStatusUpdated(data: IImageSearchStatus) {
    if (data) status.value = { ...data }
  }

  emitter.on('image_search_updated', onStatusUpdated)
  onUnmounted(() => emitter.off('image_search_updated', onStatusUpdated))

  return { status }
}
