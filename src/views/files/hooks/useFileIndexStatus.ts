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
    }
  }
  function schedule(delay: number) {
    if (!enabled.value || timer) return
    timer = setTimeout(() => { timer = undefined; void refresh() }, delay)
  }
  watch([enabled, getGraphqlContextKey], (_value, _previous, onCleanup) => {
    generation++
    clearTimeout(timer); timer = undefined
    progress.value = null; error.value = ''
    if (enabled.value) {
      const stop = graphqlSubscriptions.subscribe('fileIndexScanProgressUpdated', () => { generation++; schedule(1000) })
      const stopAttached = graphqlSubscriptions.onAttached(() => { generation++; void refresh() })
      onCleanup(() => { stop(); stopAttached() })
      void refresh()
    }
  }, { immediate: true })
  onUnmounted(() => { generation++; clearTimeout(timer) })
  return { progress, error }
}
