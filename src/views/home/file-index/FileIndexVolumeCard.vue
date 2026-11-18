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
    <footer>
      <div class="scan-summary" aria-live="polite">
        <p class="scan-info">
          <template v-if="scanProgress?.state === 'FAILED'">{{ scanProgress.error || $t('FAILED') }}</template>
          <template v-else-if="scanProgress?.state === 'RUNNING'">
            {{ scanProgress.indexed > 0 ? $t('file_index.scanned_files', { count: scanProgress.indexed.toLocaleString() }) : $t('file_index.scanning') }}
          </template>
          <template v-else-if="scanProgress?.state === 'QUEUED'">{{ $t('waiting') }}</template>
          <template v-else-if="scanProgress?.state === 'PAUSED'">{{ $t('paused') }}</template>
          <template v-else-if="volume.indexMode === 'DISABLED'">{{ $t('file_index.disabled') }}</template>
          <template v-else-if="scanProgress?.state === 'STOPPED'">{{ $t('stopped') }}</template>
          <template v-else-if="volume.lastScanStatus === 'COMPLETED'">
            {{ volume.lastScanCount === null ? $t('completed') : $t('file_index.scanned_files', { count: volume.lastScanCount.toLocaleString() }) }}
          </template>
          <template v-else-if="volume.lastScanStatus === 'FAILED'">{{ $t('FAILED') }}</template>
          <template v-else-if="volume.lastScanStatus === 'CANCELED'">{{ $t('stopped') }}</template>
          <template v-else>{{ $t('file_index.not_scanned') }}</template>
        </p>
        <p v-if="!scanActive && (volume.indexMode === 'ALLOWLIST' || volume.indexMode === 'BLOCKLIST')" class="scan-info">
          <template v-if="volume.indexMode === 'ALLOWLIST'">{{ $t('file_index.scope_selected_folders', { count: volume.includedDirs.length }) }}</template>
          <template v-else>
            {{ $t('file_index.scope_all_folders') }}<template v-if="volume.excludedDirs.length"> · {{ $t('file_index.scope_excluded_folders', { count: volume.excludedDirs.length }) }}</template>
          </template>
        </p>
      </div>
      <div class="scan-actions">
        <template v-if="scanActive && scanProgress">
          <v-text-button disabled>{{ $t(scanProgress.state === 'QUEUED' ? 'waiting' : scanProgress.state === 'PAUSED' ? 'paused' : 'file_index.scanning') }}</v-text-button>
          <v-icon-button v-if="scanProgress.state === 'RUNNING' || scanProgress.state === 'QUEUED'" v-tooltip="$t('pause')" :aria-label="$t('pause')" :disabled="scanDisabled" @click="$emit('pause')"><i-material-symbols:pause-rounded /></v-icon-button>
          <v-icon-button v-if="scanProgress.state === 'PAUSED'" v-tooltip="$t('resume')" :aria-label="$t('resume')" :disabled="scanDisabled" @click="$emit('resume')"><i-material-symbols:play-arrow-rounded /></v-icon-button>
          <v-icon-button v-tooltip="$t('stop')" :aria-label="$t('stop')" :disabled="scanDisabled" @click="$emit('stop')"><i-material-symbols:stop-rounded /></v-icon-button>
        </template>
        <v-text-button v-else-if="volume.indexMode === 'UNCONFIGURED'" :disabled="scanDisabled || !volume.isAvailable" @click="$emit('create')">
          {{ $t('file_index.create') }}
        </v-text-button>
        <template v-else>
          <v-text-button :disabled="scanDisabled || !canScanVolume(volume)" @click="$emit('scan')">{{ $t('rebuild_index') }}</v-text-button>
        </template>
      </div>
    </footer>
  </article>
</template>

<script setup lang="ts">
import { computed, onDeactivated, ref } from 'vue'
import VDropdown from '@/components/base/VDropdown.vue'
import { formatFileSize } from '@/lib/format'
import { canScanVolume, type FileIndexVolume } from '@/lib/api/file-index'
import type { IFileIndexScanProgress, IStorageMount } from '@/lib/interfaces'

const props = defineProps<{ volume: FileIndexVolume; title: string; mount?: IStorageMount; scanDisabled: boolean; scanProgress?: IFileIndexScanProgress }>()
defineEmits<{ settings: []; create: []; scan: []; pause: []; resume: []; stop: [] }>()
const scanActive = computed(() => {
  const state = props.scanProgress?.state
  return state === 'QUEUED' || state === 'RUNNING' || state === 'PAUSED'
})
const rawOpen = ref(false)
onDeactivated(() => { rawOpen.value = false })
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
  justify-content: space-between;
  gap: 8px;
}
.scan-summary {
  min-width: 0;
  overflow-wrap: anywhere;
}
.scan-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px;
  margin-inline-start: auto;
}
.scan-info,
.capacity,
.unavailable {
  color: var(--md-sys-color-on-surface-variant);
}
</style>
