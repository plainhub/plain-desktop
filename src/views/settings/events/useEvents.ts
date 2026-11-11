import { ref, shallowRef, onScopeDispose } from 'vue'
import { readEvents, type EventRecord } from './api'

export function useEvents(read = readEvents) {
  const search = ref('')
  const entries = shallowRef<EventRecord[]>([])
  const loading = ref(false)
  const error = ref('')
  const hasMore = ref(false)
  let generation = 0
  let nextOffset = 0
  const limit = 50

  async function load(reset = false) {
    if (loading.value && !reset) return
    const request = ++generation
    const offset = reset ? 0 : nextOffset
    loading.value = true
    error.value = ''
    if (reset) { entries.value = []; hasMore.value = false }
    try {
      const page = await read(search.value, offset, limit)
      if (request !== generation) return
      const seen = new Set(entries.value.map(entry => entry.id))
      entries.value = reset ? page : [...entries.value, ...page.filter(entry => !seen.has(entry.id))]
      nextOffset = offset + page.length
      hasMore.value = page.length === limit
    } catch (cause) {
      if (request === generation) error.value = cause instanceof Error ? cause.message : String(cause)
    } finally {
      if (request === generation) loading.value = false
    }
  }
  onScopeDispose(() => { generation++ })
  return { search, entries, loading, error, hasMore, load }
}
