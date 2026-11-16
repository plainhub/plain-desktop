<template>
  <article class="storage-card" :class="{ unavailable: !volume.isAvailable }">
    <header>
      <v-dropdown v-model="rawOpen" borderless>
        <template #trigger><i-lucide:hard-drive /></template>
        <pre class="view-raw">{{ volume }}</pre>
      </v-dropdown>
      <h3>{{ title }}</h3>
      <v-icon-button v-tooltip="$t('file_index.scope')" :disabled="scanDisabled" :aria-label="`${$t('file_index.scope')}: ${title}`" @click="$emit('settings')">
        <i-material-symbols:settings-outline-rounded />
      </v-icon-button>
    </header>
    <p v-if="mount && mount.totalBytes > 0" class="capacity">
      {{ $t('storage_free_total', { free: formatFileSize(mount.freeBytes), total: formatFileSize(mount.totalBytes) }) }}
    </p>
    <p class="scan-info">
      <template v-if="volume.lastScanAt">{{ $t('file_index.last_scan') }}: {{ formatDateTime(volume.lastScanAt) }}</template>
      <template v-else>{{ $t('file_index.not_scanned') }}</template>
    </p>
    <p v-if="volume.lastScanAt" class="scan-info">
      {{ $t('file_index.scan_count', { count: volume.lastScanCount?.toLocaleString() ?? '—', total: volume.lastScanTotal?.toLocaleString() ?? '—' }) }}
      <template v-if="volume.lastScanStatus"> · {{ $t(scanStatusKeys[volume.lastScanStatus]) }}</template>
    </p>
    <p v-if="progressLabel" class="scan-info" aria-live="polite">
      {{ scanProgress?.state === 'FAILED' ? scanProgress.error || $t('failed') : $t(progressLabel) }}<template v-if="scanProgress && scanProgress.state !== 'FAILED' && scanProgress.indexed > 0"> · {{ scanProgress.indexed.toLocaleString() }}<template v-if="scanProgress.total !== null"> / {{ scanProgress.total.toLocaleString() }}</template></template>
    </p>
    <footer>
      <v-text-button v-if="volume.indexMode === 'UNCONFIGURED'" :disabled="scanDisabled || !volume.isAvailable" @click="$emit('create')">
        {{ $t('file_index.create') }}
      </v-text-button>
      <template v-else>
        <p class="mode">
          {{ $t(modeKey) }}<template v-if="volume.indexMode === 'ALLOWLIST'"> · {{ volume.includedDirs.length }}</template>
          <template v-else-if="volume.indexMode === 'BLOCKLIST'"> · {{ volume.excludedDirs.length }}</template>
        </p>
        <template v-if="scanProgress && ['QUEUED', 'RUNNING', 'PAUSED'].includes(scanProgress.state)">
          <v-icon-button v-if="scanProgress.state === 'RUNNING' || scanProgress.state === 'QUEUED'" v-tooltip="$t('pause')" :aria-label="$t('pause')" :disabled="scanDisabled" @click="$emit('pause')"><i-material-symbols:pause-rounded /></v-icon-button>
          <v-icon-button v-if="scanProgress.state === 'PAUSED'" v-tooltip="$t('resume')" :aria-label="$t('resume')" :disabled="scanDisabled" @click="$emit('resume')"><i-material-symbols:play-arrow-rounded /></v-icon-button>
          <v-icon-button v-tooltip="$t('stop')" :aria-label="$t('stop')" :disabled="scanDisabled" @click="$emit('stop')"><i-material-symbols:stop-rounded /></v-icon-button>
        </template>
        <template v-else>
          <v-text-button :disabled="scanDisabled || !canScanVolume(volume)" @click="$emit('scan')">{{ $t('rebuild_index') }}</v-text-button>
          <v-text-button v-if="volume.indexMode !== 'DISABLED'" :disabled="scanDisabled || !volume.isAvailable" @click="$emit('disable')">{{ $t('file_index.disable') }}</v-text-button>
        </template>
      </template>
    </footer>
  </article>
</template>

<script setup lang="ts">
import { computed, onDeactivated, ref } from 'vue'
import VDropdown from '@/components/base/VDropdown.vue'
import { formatDateTime, formatFileSize } from '@/lib/format'
import { canScanVolume, type FileIndexVolume, type FileIndexScanStatus } from '@/lib/api/file-index'
import type { IFileIndexScanProgress, IStorageMount } from '@/lib/interfaces'

const props = defineProps<{ volume: FileIndexVolume; title: string; mount?: IStorageMount; scanDisabled: boolean; scanProgress?: IFileIndexScanProgress }>()
defineEmits<{ settings: []; create: []; disable: []; scan: []; pause: []; resume: []; stop: [] }>()
const progressLabel = computed(() => {
  const progress = props.scanProgress
  if (!progress) return ''
  if (progress.state === 'QUEUED') return 'waiting'
  if (progress.state === 'RUNNING') return 'building_file_index'
  if (progress.state === 'PAUSED') return 'paused'
  if (progress.state === 'FAILED') return 'failed'
  return ''
})
const scanStatusKeys: Record<FileIndexScanStatus, string> = { COMPLETED: 'completed', CANCELED: 'stopped', FAILED: 'failed' }
const rawOpen = ref(false)
onDeactivated(() => { rawOpen.value = false })
const modeKey = computed(
  () =>
    `file_index.${props.volume.indexMode === 'ALLOWLIST' ? 'allowlist' : props.volume.indexMode === 'BLOCKLIST' ? 'blocklist' : 'disabled'}`
)
</script>

<style scoped>
.storage-card {
  min-width: 0;
  padding: 8px;
  border-radius: 8px;
  background: var(--md-sys-color-surface-container-high);
}
header {
  display: flex;
  align-items: center;
  gap: 8px;
}
header :deep(.v-dropdown-container svg) {
  width: 16px;
  height: 16px;
}
h3 {
  flex: 1;
  min-width: 0;
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.75rem;
  font-weight: 500;
}
p {
  margin: 4px 0 0;
  font-size: 0.75rem;
}
footer {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
}
footer p {
  margin: 0;
  margin-inline-end: auto;
}
.scan-info,
.capacity,
.unavailable {
  color: var(--md-sys-color-on-surface-variant);
}
.mode {
  color: var(--md-sys-color-primary);
}
.unavailable .mode {
  color: inherit;
}
</style>
