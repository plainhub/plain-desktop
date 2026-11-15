import { ref, watch, type Ref } from 'vue'
import { usePageScope } from '@/hooks/page-scope'
import { createRefreshScheduler } from '@/lib/refresh-scheduler'
import { gqlFetch, getGraphqlContextKey } from '@/lib/api/gql-client'
import { fileIndexScanProgressGQL } from '@/lib/api/query'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import type { IFileIndexScanProgress } from '@/lib/interfaces'

export function useFileIndexStatus(enabled: Ref<boolean>) {
  const progress = ref<IFileIndexScanProgress | null>(null)
  const error = ref('')
  let generation = 0
  let pending = 0
  const scheduler = createRefreshScheduler(() => { void refresh() }, { delay: 1000, maxWait: 1000 })

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
  usePageScope(scope => {
    const stopWatch = watch([enabled, getGraphqlContextKey], (_value, _previous, onCleanup) => {
      generation++
      scheduler.dispose()
      progress.value = null; error.value = ''
      if (enabled.value) {
        const unsubscribe = graphqlSubscriptions.subscribe('fileIndexScanProgressUpdated', () => { generation++; scheduler.schedule() })
        const stopAttached = graphqlSubscriptions.onAttached(() => { generation++; void refresh() })
        onCleanup(() => { unsubscribe(); stopAttached() })
        void refresh()
      }
    }, { immediate: true })
    scope.onDispose(() => { generation++; scheduler.dispose(); stopWatch() })
  })
  return { progress, error }
}
