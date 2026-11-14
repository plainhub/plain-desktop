import { ref, watch, onUnmounted, type Ref } from 'vue'
import { gqlFetch, getGraphqlContextKey } from '@/lib/api/gql-client'
import { fileIndexScanProgressGQL } from '@/lib/api/query'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import type { IFileIndexScanProgress } from '@/lib/interfaces'

export function useFileIndexStatus(enabled: Ref<boolean>) {
  const progress = ref<IFileIndexScanProgress | null>(null)
  const error = ref('')
  let generation = 0
  let pending = 0
  let timer: ReturnType<typeof setTimeout> | undefined

  async function refresh() {
    const current = generation
    if (!enabled.value || pending === current) return
    pending = current
    try {
      const response = await gqlFetch<{ fileIndexScanProgress: IFileIndexScanProgress }>(fileIndexScanProgressGQL, undefined, { fresh: true })
      if (response.errors?.length) throw new Error(response.errors[0].message)
      if (current === generation) { progress.value = response.data.fileIndexScanProgress; error.value = '' }
    } catch (cause) {
      if (current === generation) error.value = cause instanceof Error ? cause.message : String(cause)
    } finally {
      if (pending === current) pending = 0
      if (current === generation && enabled.value && progress.value?.state === 'RUNNING') schedule(5000)
    }
  }
  function schedule(delay: number) {
    if (!enabled.value || timer) return
    timer = setTimeout(() => { timer = undefined; void refresh() }, delay)
  }
  watch([enabled, getGraphqlContextKey], () => {
    generation++
    clearTimeout(timer); timer = undefined
    progress.value = null; error.value = ''
    if (enabled.value) void refresh()
  }, { immediate: true })
  const stop = graphqlSubscriptions.subscribe('fileIndexScanProgressUpdated', () => schedule(1000))
  const stopAttached = graphqlSubscriptions.onAttached(() => { void refresh() })
  onUnmounted(() => { generation++; clearTimeout(timer); stop(); stopAttached() })
  return { progress, error }
}
