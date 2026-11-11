<template>
  <Teleport v-if="isActive" to="#header-end-slot" defer>
    <v-icon-button v-tooltip="$t('refresh')" :aria-label="$t('refresh')" :loading="loading" @click="load(true)"><i-material-symbols:refresh-rounded /></v-icon-button>
  </Teleport>
  <div class="scroll-content events-view">
    <h1>{{ $t('events') }}</h1>
    <v-text-field variant="filled" v-model="search" :placeholder="$t('search')" :aria-label="$t('search')" />
    <p v-if="error" role="alert">{{ error }}</p>
    <v-circular-progress v-if="loading" indeterminate />
    <p v-else-if="!entries.length && !error">{{ $t('no_data') }}</p>
    <article v-for="entry in entries" :key="entry.id" class="card event-row">
      <div class="event-heading"><field-id :id="entry.id" :raw="entry" /><strong>{{ $t(`event_type.${entry.type}`) }}</strong><time :datetime="entry.createdAt">{{ formatDateTime(entry.createdAt) }}</time></div>
      <p>{{ entry.type === 'LOGIN_FAILED' && te(`event_reason.${entry.message}`) ? $t(`event_reason.${entry.message}`) : entry.message }}</p>
      <p v-if="entry.clientId" class="event-client">{{ $t('event_client') }}: {{ entry.clientId }}</p>
    </article>
    <v-filled-button v-if="hasMore" :disabled="loading" @click="load()">{{ $t('events_load_more') }}</v-filled-button>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onActivated, onDeactivated } from 'vue'
import { useI18n } from 'vue-i18n'
import { formatDateTime } from '@/lib/format'
import { getGraphqlContextKey } from '@/lib/api/gql-client'
import { useEvents } from './events/useEvents'
const { te } = useI18n()
const { search, entries, loading, error, hasMore, load } = useEvents()
const isActive = ref(false)
onMounted(() => load(true))
onActivated(() => { isActive.value = true; if (!loading.value) load(true) })
onDeactivated(() => { isActive.value = false })
watch(search, () => load(true))
watch(() => getGraphqlContextKey(), () => load(true))
</script>

<style scoped>
.events-view { display: flex; flex-direction: column; gap: 16px; padding: 24px; }
.event-row { padding: 16px; margin: 0; }
.event-heading { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 8px; }
.event-row p { margin: 8px 0 0; overflow-wrap: anywhere; }
.event-client, time { color: var(--md-sys-color-on-surface-variant); }
</style>
