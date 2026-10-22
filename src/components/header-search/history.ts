import { computed, ref } from 'vue'
import { readSavedValue, saveValue } from '@/lib/saved-values'

export function useSearchHistory(opts: {
  max: number
  decodeBase64: (s: string) => string
  formatLabel: (decoded: string) => string
}) {
  const historyQ = ref<string[]>([])
  let edited = false
  let writes = Promise.resolve()
  void readSavedValue<string[]>('recent_searches', []).then((items) => {
    if (!edited) historyQ.value = items
  }).catch(console.error)

  function setHistory(next: string[]) {
    edited = true
    historyQ.value = next
    writes = writes.then(() => saveValue('recent_searches', next)).catch(console.error)
  }

  function rememberHistoryDecoded(q: string) {
    const normalized = String(q ?? '').trim()
    if (!normalized) return

    const list = [...(historyQ.value ?? [])]
    const next = [normalized, ...list.filter((it) => it !== normalized)].slice(0, opts.max)
    setHistory(next)
  }

  function rememberHistoryBase64(qBase64: string) {
    if (!qBase64) return
    try {
      rememberHistoryDecoded(opts.decodeBase64(qBase64))
    } catch {
      // ignore
    }
  }

  function deleteHistoryItem(q: string) {
    if (!q) return
    setHistory((historyQ.value ?? []).filter((it) => it !== q))
  }

  function clearHistoryForPage() {
    setHistory([])
  }

  const historyValueOptions = computed(() => {
    return (historyQ.value ?? [])
      .map((q) => ({ value: q, label: opts.formatLabel(q) || q }))
      .filter((it) => it.value)
  })

  return {
    historyQ,
    historyValueOptions,
    rememberHistoryDecoded,
    rememberHistoryBase64,
    deleteHistoryItem,
    clearHistoryForPage,
  }
}
