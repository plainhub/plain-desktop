<template>
  <section class="index-status" aria-live="polite">
    <p>{{ $t('index_search_scope_hint') }}</p>
    <p v-if="status">{{ $t(`index_state_${status.status}`) }} · {{ status.processedFiles.toLocaleString() }}</p>
    <p v-else>{{ $t('index_state_pending') }}</p>
    <p v-if="error || status?.error" class="error">{{ error || status?.error }}</p>
    <details v-if="status?.sources.length">
      <summary>{{ $t('index_coverage') }}</summary>
      <p v-for="source in status.sources" :key="source.path">{{ source.path }} · {{ $t(`index_source_${source.state}`) }}<span v-if="source.error"> · {{ source.error }}</span></p>
    </details>
  </section>
</template>

<script setup lang="ts">
import { ref, onUnmounted, onActivated, onDeactivated } from 'vue'
import { gqlFetch } from '@/lib/api/gql-client'
import { prefsGQL } from '@/lib/api/query'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'

interface Coverage { path: string; state: 'pending' | 'scanning' | 'completed' | 'excluded' | 'failed'; error: string | null }
interface Progress { status: 'running' | 'paused' | 'cancelling' | 'cancelled' | 'completed' | 'failed'; processedFiles: number; sources: Coverage[]; error: string | null }
const status = ref<Progress | null>(null)
const error = ref('')
let disposed = false
let active = true
let loading = false
let timer: ReturnType<typeof setTimeout> | undefined

async function refresh() {
  if (disposed || !active || loading) return
  loading = true
  try {
    const response = await gqlFetch<{ prefs: { file_index_progress: Progress | null } }>(prefsGQL, undefined, { fresh: true })
    if (response.errors?.length) throw new Error(response.errors[0].message)
    if (!disposed) { status.value = response.data.prefs.file_index_progress; error.value = '' }
  } catch (cause) {
    if (!disposed) error.value = cause instanceof Error ? cause.message : String(cause)
  } finally {
    loading = false
    if (!disposed && active && status.value?.status === 'running') timer = setTimeout(() => { timer = undefined; void refresh() }, 5000)
  }
}

const stop = graphqlSubscriptions.subscribe('mediaScanProgress', () => {
  if (active && !loading && !timer) timer = setTimeout(() => { timer = undefined; void refresh() }, 1000)
})
const stopAttached = graphqlSubscriptions.onAttached(() => { void refresh() })
void refresh()
onActivated(() => { active = true; void refresh() })
onDeactivated(() => { active = false; clearTimeout(timer); timer = undefined })
onUnmounted(() => { disposed = true; stop(); stopAttached(); clearTimeout(timer) })
</script>

<style scoped>
.index-status { padding: 8px 16px; color: var(--md-sys-color-on-surface-variant); }
p { margin: 0 0 8px; overflow-wrap: anywhere; }
.error { color: var(--md-sys-color-error); }
</style>
