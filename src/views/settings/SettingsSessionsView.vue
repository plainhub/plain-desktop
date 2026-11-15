<template>
  <Teleport v-if="isActive" to="#header-end-slot" defer>
    <v-icon-button v-tooltip="$t('refresh')" :aria-label="$t('refresh')" :loading="loading" :disabled="busy" @click="load"><i-material-symbols:refresh-rounded /></v-icon-button>
  </Teleport>
  <div class="scroll-content sessions-view">
    <div class="session-tabs">
      <v-filter-chip :label="$t('session_management.title')" :selected="tab === 'LOGIN'" :aria-pressed="tab === 'LOGIN'" @click="tab = 'LOGIN'" />
      <v-filter-chip :label="$t('session_management.api_tokens')" :selected="tab === 'API_TOKEN'" :aria-pressed="tab === 'API_TOKEN'" @click="tab = 'API_TOKEN'" />
      <v-filled-button v-if="tab === 'API_TOKEN'" :disabled="busy || loading" @click="editName(null)"><i-lucide:plus />{{ $t('session_management.create_token') }}</v-filled-button>
    </div>
    <p v-if="error" role="alert">{{ displayError }}</p>
    <p v-if="onlineError" role="alert">{{ onlineError }}</p>
    <v-circular-progress v-if="loading" indeterminate />
    <p v-if="!loading && !visibleEntries.length && !error">{{ $t('no_data') }}</p>
    <SessionCard v-for="entry in visibleEntries" :key="entry.clientId" :entry="entry" :online="onlineIds === null ? undefined : onlineIds.has(entry.clientId)" :busy="busy || loading" @rename="editName(entry)" @revoke="revoke(entry.clientId)" />
  </div>
  <CreatedTokenDialog v-if="createdToken" :credential="createdToken" @close="createdToken = null" />
</template>

<script setup lang="ts">
import { computed, ref, shallowRef, watch, onMounted, onActivated, onDeactivated } from 'vue'
import { useI18n } from 'vue-i18n'
import { getGraphqlContextKey } from '@/lib/api/gql-client'
import { isLocalMode } from '@/lib/device/local-mode'
import { subscribeOnlineClients } from '@/lib/device/sessions'
import { useSessions } from './sessions/useSessions'
import type { CreatedApiToken, SessionRecord } from './sessions/api'
import SessionCard from './sessions/SessionCard.vue'
import CreatedTokenDialog from './sessions/CreatedTokenDialog.vue'
import EditValueModal from '@/components/EditValueModal.vue'
import { openModal, type Modal } from '@/components/modal'

const { t, te } = useI18n()
const { entries, loading, busy, error, load, reset, rename, revoke, create } = useSessions()
const isActive = ref(false)
const tab = ref<'LOGIN' | 'API_TOKEN'>('LOGIN')
let nameModal: Modal | undefined
const createdToken = shallowRef<CreatedApiToken | null>(null)
const onlineIds = shallowRef<Set<string> | null>(null)
const onlineError = ref('')
const displayError = computed(() => te(error.value) ? t(error.value) : error.value)
const visibleEntries = computed(() => entries.value.filter(entry => entry.type === tab.value).sort((a, b) => Number(onlineIds.value?.has(b.clientId) ?? false) - Number(onlineIds.value?.has(a.clientId) ?? false)))

onMounted(() => { isActive.value = true })
onActivated(() => { isActive.value = true })
onDeactivated(() => { isActive.value = false })
watch(() => [isActive.value, getGraphqlContextKey()], (_, __, onCleanup) => {
  reset()
  createdToken.value = null
  onlineIds.value = null
  onlineError.value = ''
  if (!isActive.value) return
  void load()
  let disposed = false
  let release: (() => Promise<void>) | undefined
  onCleanup(() => {
    disposed = true
    reset()
    if (nameModal && !nameModal.closed.value) void nameModal.close().catch(console.error)
    nameModal = undefined
    if (release) void release().catch(console.error)
  })
  if (isLocalMode()) {
    void subscribeOnlineClients(ids => {
      if (!disposed) onlineIds.value = new Set(ids)
    }).then(close => {
      if (disposed) void close().catch(console.error)
      else release = close
    }).catch(cause => {
      if (!disposed) onlineError.value = `${t('session_management.online_failed')}: ${cause instanceof Error ? cause.message : String(cause)}`
    })
  }
})

async function editName(entry: SessionRecord | null) {
  error.value = ''
  const context = getGraphqlContextKey()
  const modal = await openModal(EditValueModal, {
    title: t(entry ? 'rename' : 'session_management.create_token'),
    placeholder: t('name'),
    value: entry ? entry.clientName || [entry.osName, entry.osVersion].filter(Boolean).join(' ') : '',
    allowEmpty: !!entry,
    borderless: true,
    error: displayError,
    getVariables: (value: string) => ({ name: value.trim() }),
    mutation: () => {
      let done!: () => void
      return {
        loading: busy,
        onDone: (callback: () => void) => { done = callback },
        mutate: async ({ name }: { name: string }) => {
          if (!isActive.value || context !== getGraphqlContextKey()) return
          const result = entry ? await rename(entry.clientId, name) : await create(name)
          if (result) {
            if (typeof result !== 'boolean') createdToken.value = result
            done()
          }
        },
      }
    },
  })
  if (!isActive.value || context !== getGraphqlContextKey()) await modal.close()
  else nameModal = modal
}
</script>

<style scoped>
.sessions-view { display: flex; flex-direction: column; gap: 16px; padding: 24px; }
.session-tabs { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; }
.session-tabs :deep(.v-filter-chip) { border: 0; }
</style>
