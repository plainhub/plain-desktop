import { ref, watch, onUnmounted } from 'vue'
import { gqlFetch } from '@/lib/api/gql-client'
import { prefsGQL } from '@/lib/api/query'
import { setPrefGQL } from '@/lib/api/mutation'
import { debounce } from '@/lib/array'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { applySnapshot } from '@/lib/prefs'
import type { useMainStore } from '@/stores/main'

export function useRemotePrefs(store: ReturnType<typeof useMainStore>) {
  const enabled = ref(false)
  let applying = false
  let writes = Promise.resolve()
  const save = debounce((value: string[]) => {
    writes = writes.then(async () => {
      const result = await gqlFetch(setPrefGQL, { key: 'desktop_home_features', value }, { dedupe: false })
      if (result.errors?.length) throw new Error(result.errors[0].message)
    }).catch((error) => { console.warn('Failed to save desktop home features', error) })
  }, 500)
  watch(() => store.homeFeatures, (value) => {
    if (enabled.value && !applying) save([...value])
  }, { deep: true, flush: 'sync' })

  async function refresh() {
    try {
      const result = await gqlFetch<{ prefs: Record<string, unknown> }>(prefsGQL, undefined, { fresh: true })
      if (result.errors?.length) throw new Error(result.errors[0].message)
      if (!__IS_TAURI__) await applySnapshot(result.data.prefs)
      const value = result.data.prefs.desktop_home_features
      if (Array.isArray(value) && value.every((item) => typeof item === 'string')) {
        applying = true
        store.homeFeatures = value
        applying = false
      }
      enabled.value = true
    } catch (error) {
      console.warn('Failed to load preferences', error)
    }
  }
  void refresh()
  const stop = graphqlSubscriptions.subscribe('preferencesChanged', refresh)
  const stopAttached = graphqlSubscriptions.onAttached(refresh)
  onUnmounted(() => { stop(); stopAttached() })
  return enabled
}
