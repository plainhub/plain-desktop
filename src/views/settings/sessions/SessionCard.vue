<template>
  <article class="card session-card">
    <header>
      <v-dropdown v-model="rawOpen" borderless>
        <template #trigger>
          <v-icon-button :aria-label="$t('session_management.view_raw')" :aria-expanded="rawOpen" aria-haspopup="dialog">
            <i-material-symbols:key-rounded v-if="entry.type === 'API_TOKEN'" />
            <i-lucide:laptop v-else />
          </v-icon-button>
        </template>
        <pre class="view-raw" role="dialog" :aria-label="$t('session_management.view_raw')" tabindex="0" @keydown.esc.stop="rawOpen = false">{{ entry }}</pre>
      </v-dropdown>
      <h2>{{ entry.clientName || osDisplay || $t('unknown') }}</h2>
      <v-icon-button :aria-label="$t('rename')" :disabled="busy" @click="$emit('rename')"><i-lucide:pencil /></v-icon-button>
      <span v-if="online !== undefined" class="status">{{ $t(online ? 'session_management.online' : 'session_management.offline') }}</span>
    </header>
    <dl>
      <div v-if="osDisplay"><dt>{{ $t('os_name') }}</dt><dd>{{ osDisplay }}</dd></div>
      <div v-if="entry.clientIp"><dt>{{ $t('ip_address') }}</dt><dd>{{ entry.clientIp }}</dd></div>
      <div v-if="entry.appName || entry.appVersion"><dt>{{ $t('session_management.app') }}</dt><dd>{{ [entry.appName, entry.appVersion].filter(Boolean).join(' ') }}</dd></div>
      <div><dt>{{ $t('created_at') }}</dt><dd><time :datetime="entry.createdAt">{{ formatDateTime(entry.createdAt) }}</time></dd></div>
      <div v-if="entry.lastSeenAt && !online"><dt>{{ $t('session_management.last_active') }}</dt><dd><time :datetime="entry.lastSeenAt">{{ formatDateTime(entry.lastSeenAt) }}</time></dd></div>
    </dl>
    <div class="credential"><span>{{ $t('client_id') }}</span><code>{{ entry.clientId }}</code><v-copy-button :text="entry.clientId" /></div>
    <div class="actions">
      <v-text-button class="danger" :disabled="busy" @click="confirming = true">{{ $t(entry.type === 'API_TOKEN' ? 'session_management.revoke_token' : 'session_management.revoke_session') }}</v-text-button>
    </div>
    <inline-delete-confirm v-if="confirming" :name="entry.clientName || osDisplay || $t('unknown')" message-key="session_management.confirm_revoke" :loading="busy" @cancel="confirming = false" @confirm="$emit('revoke')" />
  </article>
</template>

<script setup lang="ts">
import { computed, ref, onDeactivated } from 'vue'
import VDropdown from '@/components/base/VDropdown.vue'
import { formatDateTime } from '@/lib/format'
import type { SessionRecord } from './api'
const props = defineProps<{ entry: SessionRecord; online?: boolean; busy: boolean }>()
defineEmits<{ rename: []; revoke: [] }>()
const osDisplay = computed(() => [props.entry.osName, props.entry.osVersion].filter(Boolean).join(' '))
const rawOpen = ref(false)
const confirming = ref(false)
onDeactivated(() => { rawOpen.value = false })
</script>

<style scoped>
.session-card { padding: 24px; margin: 0; }
header { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
h2 { margin: 0; font-size: 1rem; font-weight: 500; overflow-wrap: anywhere; }
.status { margin-inline-start: auto; color: var(--md-sys-color-primary); }
dl { display: grid; grid-template-columns: repeat(auto-fit, minmax(192px, 1fr)); gap: 16px; margin: 16px 0; }
dt, .credential > span { color: var(--md-sys-color-on-surface-variant); }
dd { margin: 4px 0 0; overflow-wrap: anywhere; }
.credential { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-top: 8px; }
code { overflow-wrap: anywhere; }
.actions { display: flex; justify-content: flex-end; margin-top: 16px; }
.session-card .danger { color: var(--md-sys-color-error); }
.session-card :deep(.inline-delete-confirm) { padding: 16px; gap: 16px; }
.session-card :deep(.v-outlined-button) { border: 0; }
</style>
