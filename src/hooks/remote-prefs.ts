import { GqlError } from '@/lib/api/gql-error'
import type { AppFeatureType } from '@/lib/app-feature.generated'
import { ref, watch, onUnmounted } from 'vue'
import { gqlFetch } from '@/lib/api/gql-client'
import { setPrefGQL } from '@/lib/api/mutation'
import { debounce } from '@/lib/array'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { applySnapshot, preferenceKeys, preferencesQuery } from '@/lib/prefs/core'
import { isLocalMode } from '@/lib/device/local-mode'
import { apply as applyAppearance } from '@/lib/prefs/appearance'
import { DEFAULT_RAIL_FEATURES, type useMainStore } from '@/stores/main'
import { DEFAULT_HOME_FEATURES } from '@/views/home/features'

const featurePrefs = [
  { field: 'homeFeatures', key: 'desktop_home_features', defaults: DEFAULT_HOME_FEATURES },
  { field: 'railFeatures', key: 'desktop_rail_features', defaults: DEFAULT_RAIL_FEATURES },
] as const

export function useRemotePrefs(store: ReturnType<typeof useMainStore>) {
  const enabled = ref(false)
  let applying = false
  let writes = Promise.resolve()
  const savePreference = (key: string) => debounce((value: unknown) => {
    writes = writes.then(async () => {
      const result = await gqlFetch(setPrefGQL, { key, value }, { dedupe: false })
      if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
    }).catch((error) => { console.warn(`Failed to save ${key}`, error) })
  }, 500)
  for (const { field, key } of featurePrefs) {
    const save = savePreference(key)
    watch(() => store[field], (value) => {
      if (enabled.value && !applying) save([...value])
    }, { deep: true, flush: 'sync' })
  }

  const saveUploadDirs = savePreference('desktop_upload_dirs')
  watch(() => store.uploadDirs, (value) => {
    if (enabled.value && !applying) saveUploadDirs({ ...value })
  }, { deep: true, flush: 'sync' })

  async function refresh() {
    try {
      const result = await gqlFetch<{ prefs: Record<string, unknown> }>(preferencesQuery, { keys: isLocalMode() ? [...preferenceKeys, 'clipboard_directions', 'remote_access', 'mdns_hostname'] : preferenceKeys }, { fresh: true })
      if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
      applySnapshot(result.data.prefs)
      applying = true
      for (const { field, key, defaults } of featurePrefs) {
        const value = result.data.prefs[key]
        store[field] = Array.isArray(value) && value.every((item) => typeof item === 'string')
          ? value as AppFeatureType[] : [...defaults]
      }
      const uploadDirs = result.data.prefs.desktop_upload_dirs
      store.uploadDirs = uploadDirs && typeof uploadDirs === 'object' && !Array.isArray(uploadDirs)
        && Object.values(uploadDirs).every((value) => typeof value === 'string')
        ? { ...uploadDirs as Record<string, string> } : {}
      applying = false
      enabled.value = true
      if (__IS_TAURI__) await applyAppearance()
    } catch (error) {
      console.warn('Failed to load preferences', error)
    }
  }
  void refresh()
  const stop = graphqlSubscriptions.subscribe('prefsChanged', refresh)
  const stopAttached = graphqlSubscriptions.onAttached(refresh)
  onUnmounted(() => { stop(); stopAttached() })
  return enabled
}
