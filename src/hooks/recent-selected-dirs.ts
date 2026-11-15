import { onMounted, ref } from 'vue'
import { bindGraphqlClient, getGraphqlContextKey } from '@/lib/api/gql-client'
import { preferencesQuery, set } from '@/lib/prefs/core'
import { directoryPathKey, normalizeDirectoryPath } from '@/lib/directory-picker-path'

const key = 'recent_selected_dirs'
const limit = 8

export function addRecentDirectory(recent: string[], path: string): string[] {
  const normalized = normalizeDirectoryPath(path)
  if (!normalized) return recent
  const id = directoryPathKey(normalized)
  return [normalized, ...recent.filter(item => directoryPathKey(item) !== id)].slice(0, limit)
}

export function useRecentSelectedDirs() {
  const recentDirs = ref<string[]>([])
  let context: string
  let loaded: Promise<boolean> | undefined

  onMounted(() => {
    context = getGraphqlContextKey()
    const request = bindGraphqlClient()
    loaded = request<{ prefs: Record<string, unknown> }>(preferencesQuery, { keys: [key] }).then(result => {
      if (result.errors?.length || context !== getGraphqlContextKey()) return false
      const value = result.data?.prefs?.[key]
      recentDirs.value = Array.isArray(value) ? value.filter((path): path is string => typeof path === 'string').slice(0, limit) : []
      return true
    }).catch(error => {
      console.error('Read recent directories failed', error)
      return false
    })
  })

  async function remember(path: string) {
    if (!await loaded || context !== getGraphqlContextKey()) return
    const next = addRecentDirectory(recentDirs.value, path)
    recentDirs.value = next
    await set(key, next).catch(error => console.error('Save recent directories failed', error))
  }

  return { recentDirs, remember }
}
