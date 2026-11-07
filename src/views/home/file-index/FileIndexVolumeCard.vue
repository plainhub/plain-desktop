<template>
  <article class="storage-card" :class="{ unavailable: !volume.isAvailable }">
    <header>
      <i-lucide:hard-drive aria-hidden="true" />
      <h3>{{ title }}</h3>
      <v-icon-button v-tooltip="$t('file_index.scope')" :aria-label="`${$t('file_index.scope')}: ${title}`" @click="$emit('settings')">
        <i-material-symbols:settings-outline-rounded />
      </v-icon-button>
    </header>
    <p v-if="mount && mount.totalBytes > 0" class="capacity">
      {{ $t('storage_free_total', { free: formatFileSize(mount.freeBytes), total: formatFileSize(mount.totalBytes) }) }}
    </p>
    <footer>
      <v-text-button v-if="volume.indexMode === 'UNCONFIGURED'" :disabled="scanDisabled || !volume.isAvailable" @click="$emit('settings')">
        {{ $t('file_index.create') }}
      </v-text-button>
      <template v-else>
        <p class="mode">
          {{ $t(modeKey) }}<template v-if="volume.indexMode === 'ALLOWLIST'"> · {{ volume.includedDirs.length }}</template>
          <template v-else-if="volume.indexMode === 'BLOCKLIST'"> · {{ volume.excludedDirs.length }}</template>
        </p>
        <v-text-button :disabled="scanDisabled || !canScanVolume(volume)" @click="$emit('scan')">{{ $t('rebuild_index') }}</v-text-button>
      </template>
    </footer>
  </article>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { formatFileSize } from '@/lib/format'
import { canScanVolume, type FileIndexVolume } from '@/lib/api/file-index'
import type { IStorageMount } from '@/lib/interfaces'

const props = defineProps<{ volume: FileIndexVolume; title: string; mount?: IStorageMount; scanDisabled: boolean }>()
defineEmits<{ settings: []; scan: [] }>()
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
header > svg {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
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
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
}
footer p {
  margin: 0;
  margin-inline-end: auto;
}
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
