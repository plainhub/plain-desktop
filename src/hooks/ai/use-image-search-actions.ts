import { ref, onUnmounted } from 'vue'
import emitter from '@/plugins/eventbus'
import { importImageSearchModelGQL, enableImageSearchGQL, disableImageSearchGQL, cancelImageModelDownloadGQL, startImageIndexGQL, cancelImageIndexGQL, initMutation } from '@/lib/api/mutation'
import type { IImageSearchStatus } from '@/lib/interfaces'

export function useImageSearchActions() {
  const enableLoading = ref(false)
  const disableLoading = ref(false)
  const cancelDownloadLoading = ref(false)
  const startIndexLoading = ref(false)
  const cancelIndexLoading = ref(false)

  const { mutate: mutateImport } = initMutation({ document: importImageSearchModelGQL })
  const { mutate: mutateEnable } = initMutation({ document: enableImageSearchGQL })
  const { mutate: mutateDisable } = initMutation({ document: disableImageSearchGQL })
  const { mutate: mutateCancelDownload } = initMutation({ document: cancelImageModelDownloadGQL })
  const { mutate: mutateStartIndex } = initMutation({ document: startImageIndexGQL })
  const { mutate: mutateCancelIndex } = initMutation({ document: cancelImageIndexGQL })

  function onStatusUpdated(data: IImageSearchStatus) {
    if (!data) return
    const s = data.status
    if (s) enableLoading.value = false
    if (s === 'UNAVAILABLE') {
      disableLoading.value = false
      cancelDownloadLoading.value = false
    }
    if (data.isIndexing) startIndexLoading.value = false
    else cancelIndexLoading.value = false
  }

  emitter.on('image_search_updated', onStatusUpdated)
  onUnmounted(() => emitter.off('image_search_updated', onStatusUpdated))

  async function dispatch(mutate: (variables: object) => Promise<unknown>, loading: typeof enableLoading, variables: object = {}) {
    loading.value = true
    if (!(await mutate(variables))) loading.value = false
  }
  function importModel() {
    return dispatch(mutateImport, enableLoading)
  }
  function enable() {
    return dispatch(mutateEnable, enableLoading)
  }
  function disable() {
    return dispatch(mutateDisable, disableLoading)
  }
  function cancelDownload() {
    return dispatch(mutateCancelDownload, cancelDownloadLoading)
  }
  function startIndex(force = false) {
    return dispatch(mutateStartIndex, startIndexLoading, { force })
  }
  function cancelIndex() {
    return dispatch(mutateCancelIndex, cancelIndexLoading)
  }

  return { importModel, enable, disable, cancelDownload, startIndex, cancelIndex, enableLoading, disableLoading, cancelDownloadLoading, startIndexLoading, cancelIndexLoading }
}
