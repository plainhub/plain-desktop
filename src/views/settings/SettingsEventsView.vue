<template>
  <Teleport v-if="isActive" to="#header-end-slot" defer>
    <v-icon-button v-tooltip="$t('refresh')" :aria-label="$t('refresh')" :loading="loading" @click="reload"><i-material-symbols:refresh-rounded /></v-icon-button>
  </Teleport>
  <div ref="scrollContainer" class="scroll-content events-view" @scroll.passive="loadNextPage">
    <ol v-if="entries.length" class="event-timeline">
      <li v-for="(entry, index) in entries" :key="entry.id" class="timeline-entry" :class="{ 'has-date': hasDate(index) }">
        <p v-if="hasDate(index)" class="event-date subtle">{{ formatDate(entry.createdAt) }}</p>
        <EventCard :entry="entry" />
      </li>
    </ol>
    <p v-if="error" role="alert">{{ error }}</p>
    <v-circular-progress v-if="loading" indeterminate />
    <p v-else-if="!entries.length && !error">{{ $t('no_data') }}</p>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onActivated, onDeactivated } from 'vue'
import { useRoute } from 'vue-router'
import { parseCurrentFields } from '@/components/header-search/q'
import { formatDate } from '@/lib/format'
import { getGraphqlContextKey } from '@/lib/api/gql-client'
import { useEvents } from './events/useEvents'
import EventCard from './events/EventCard.vue'
const route = useRoute()
const { search, entries, loading, error, hasMore, load } = useEvents()
const isActive = ref(false)
const scrollContainer = ref<HTMLElement | null>(null)
function hasDate(index: number) {
  return index === 0 || formatDate(entries.value[index].createdAt) !== formatDate(entries.value[index - 1].createdAt)
}
function loadNextPage() {
  const element = scrollContainer.value
  if (!element || !isActive.value || loading.value || error.value || !hasMore.value) return
  if (element.scrollTop + element.clientHeight >= element.scrollHeight - 200) void load()
}
function reload() {
  search.value = parseCurrentFields(typeof route.query.q === 'string' ? route.query.q : '').find(field => field.name === 'text')?.value ?? ''
  if (scrollContainer.value) scrollContainer.value.scrollTop = 0
  void load(true)
}
function activate() {
  if (isActive.value) return
  isActive.value = true
  reload()
}
onMounted(activate)
onActivated(activate)
onDeactivated(() => { isActive.value = false })
watch(() => [route.query.q, getGraphqlContextKey()], () => {
  if (isActive.value && route.path === '/settings/events') reload()
})
watch([entries, loading], loadNextPage, { flush: 'post' })
</script>

<style scoped>
.events-view { --event-axis-center: 88px; display: flex; flex-direction: column; gap: 24px; padding: 24px; }
.event-timeline { list-style: none; margin: 0; padding: 0; }
.timeline-entry { --event-marker-offset: 16px; position: relative; }
.timeline-entry.has-date { --event-marker-offset: 64px; }
.timeline-entry::before { content: ''; position: absolute; inset-inline-start: var(--event-axis-center); transform: translateX(-50%); top: 0; bottom: 0; width: 4px; background: var(--md-sys-color-on-surface-variant); opacity: 0.12; }
.timeline-entry:first-child::before { top: var(--event-marker-offset); }
.timeline-entry:last-child::before { bottom: auto; height: var(--event-marker-offset); }
.timeline-entry:only-child::before { display: none; }
.event-date { margin: 0 0 24px 112px; line-height: 24px; font-weight: 500; }
@media (max-width: 600px) {
  .events-view { --event-axis-center: calc(48px + 8px + 4px); padding: 16px; }
  .event-date { margin-inline-start: 72px; }
}
</style>
