<template>
  <article class="event-card">
    <time class="subtle nowrap" :datetime="entry.createdAt"><span v-tooltip="formatDateTimeFull(entry.createdAt)">{{ formatTime(entry.createdAt) }}</span></time>
    <span class="event-dot subtle" :class="{ 'event-error': failed }" aria-hidden="true" />
    <div class="event-content">
      <v-dropdown v-model="rawOpen" strategy="below" @opened="focusRaw">
        <template #trigger>
          <h2><span ref="titleElement" role="button" tabindex="0" :aria-expanded="rawOpen" aria-haspopup="dialog" @keydown.enter.prevent="titleElement?.click()" @keydown.space.prevent="titleElement?.click()">{{ $t(`event_type.${entry.type}`) }}</span></h2>
        </template>
        <pre ref="rawPopup" class="view-raw event-raw" role="dialog" :aria-label="$t(`event_type.${entry.type}`)" tabindex="-1" @keydown.esc.stop="closeRaw">{{ JSON.stringify(entry, null, 2) }}</pre>
      </v-dropdown>
      <p v-if="entry.clientName" class="event-client">{{ entry.clientName }}</p>
      <div v-if="entry.appName || entry.appVersion || entry.osName || entry.osVersion || entry.clientIp" class="event-facts subtle">
        <span v-if="entry.appName || entry.appVersion">{{ [entry.appName, entry.appVersion].filter(Boolean).join(' ') }}</span>
        <span v-if="entry.osName || entry.osVersion">{{ [entry.osName, entry.osVersion].filter(Boolean).join(' ') }}</span>
        <span v-if="entry.clientIp">{{ entry.clientIp }}</span>
      </div>
      <p v-if="details" class="event-message" :class="{ 'event-error': failed }">{{ details }}</p>
    </div>
  </article>
</template>

<script setup lang="ts">
import { computed, ref, onDeactivated } from 'vue'
import { useI18n } from 'vue-i18n'
import { formatDateTimeFull, formatTime } from '@/lib/format'
import VDropdown from '@/components/base/VDropdown.vue'
import type { EventRecord } from './api'
import { eventDetails } from './display'

const props = defineProps<{ entry: EventRecord }>()
const { t } = useI18n()
const details = computed(() => eventDetails(props.entry, t))
const failed = computed(() => props.entry.type.endsWith('_FAILED'))
const rawOpen = ref(false)
const titleElement = ref<HTMLElement | null>(null)
const rawPopup = ref<HTMLElement | null>(null)
function focusRaw() {
  rawPopup.value?.focus()
}
function closeRaw() {
  rawOpen.value = false
  titleElement.value?.focus()
}
onDeactivated(() => { rawOpen.value = false })
</script>

<style scoped>
.event-card { display: grid; grid-template-columns: 64px 16px minmax(0, 1fr); gap: 16px; padding-bottom: 32px; }
time { padding-top: 4px; text-align: end; font-size: 0.75rem; line-height: 24px; }
.event-dot { width: 8px; height: 8px; margin: 12px auto 0; border-radius: 999px; background: currentColor; z-index: 1; }
.event-content { min-width: 0; overflow-wrap: anywhere; }
h2 { margin: 0; font-size: 1rem; font-weight: 500; line-height: 32px; }
.event-client, .event-message { margin: 8px 0 0; }
.event-client { font-weight: 500; }
.event-facts { display: flex; flex-wrap: wrap; gap: 8px 24px; margin-top: 4px; }
.event-error { color: var(--md-sys-color-error); }
:global(.v-dropdown-portal:has(> .event-raw)) { border: 0; }
@media (max-width: 600px) {
  .event-card { grid-template-columns: 48px 8px minmax(0, 1fr); gap: 8px; }
  .event-facts { display: grid; gap: 4px; }
}
</style>
