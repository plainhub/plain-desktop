import { ref, watch } from 'vue'
import { gqlFetch } from '@/lib/api/gql-client'
import { prefsGQL } from '@/lib/api/query'
import { setPrefGQL } from '@/lib/api/mutation'
import { debounce } from '@/lib/array'
import type { useMainStore } from '@/stores/main'

const KEYS = ['excludedDirs', 'railFeatures', 'homeFeatures'] as const

export function useRemotePrefs(store: ReturnType<typeof useMainStore>, localState: Record<string, unknown> | null) {
  const enabled = ref(false)

  let writes = Promise.resolve()
  for (const key of KEYS) {
    const save = debounce((value: string[]) => {
      writes = writes.then(async () => {
        const result = await gqlFetch(setPrefGQL, { key, value: JSON.stringify(value) }, { dedupe: false })
        if (result.errors?.length) throw new Error(result.errors[0].message)
      }).catch((error) => { console.warn(`Failed to save ${key} preference`, error) })
    }, 500)
    watch(() => store[key], (value) => {
      if (enabled.value) save([...value])
    }, { deep: true })
  }

  void (async () => {
    try {
      const result = await gqlFetch<{ prefs: Array<{ key: string; value: string }> }>(prefsGQL)
      if (result.errors?.length) throw new Error(result.errors[0].message)
      const remote = new Map(result.data.prefs.map(({ key, value }) => [key, value]))
      for (const key of KEYS) {
        const raw = remote.get(key)
        if (raw !== undefined) {
          try {
            const value: unknown = JSON.parse(raw)
            if (Array.isArray(value) && value.every((item) => typeof item === 'string')) store[key] = value
          } catch (error) {
            console.warn(`Invalid ${key} preference`, error)
          }
        } else if (Array.isArray(localState?.[key]) && localState[key].every((item: unknown) => typeof item === 'string')) {
          const value = localState[key] as string[]
          const saved = await gqlFetch(setPrefGQL, { key, value: JSON.stringify(value) }, { dedupe: false })
          if (saved.errors?.length) throw new Error(saved.errors[0].message)
        }
      }
      enabled.value = true
    } catch (error) {
      console.warn('Failed to load remote preferences', error)
    }
  })()

  return enabled
}
