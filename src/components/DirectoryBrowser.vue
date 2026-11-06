<template>
  <div class="browser">
    <aside v-if="!volume" class="browser-sidebar">
      <div v-if="loadingMounts" class="volumes-loading">
        <v-circular-progress indeterminate class="sm" />
      </div>
      <ul v-else class="browser-volumes" :aria-label="t('volumes')">
        <v-list-item
          v-for="v in sortedVolumes"
          :key="v.id"
          class="volume-item"
          role="button"
          tabindex="0"
          clickable
          :title="getStorageVolumeTitle(v, t)"
          :subtitle="mountCount(v)"
          :active="String(v.mountPoint || '') === activeRoot"
          :aria-pressed="String(v.mountPoint || '') === activeRoot"
          @click="emit('selectRoot', v.mountPoint || '/')"
          @keydown.enter.prevent="emit('selectRoot', v.mountPoint || '/')"
          @keydown.space.prevent="emit('selectRoot', v.mountPoint || '/')"
        >
          <template #start><i-lucide:hard-drive width="20" height="20" /></template>
          <template v-if="String(v.mountPoint || '') === activeRoot" #end>
            <i-material-symbols:check-rounded width="16" height="16" />
          </template>
        </v-list-item>
      </ul>
    </aside>

    <section class="browser-main">
      <div class="selection" aria-live="polite">
        <span class="subtle">{{ t('folder_picker_current') }}</span>
        <span v-if="loadingMounts">{{ t('loading') }}</span>
        <VBreadcrumb v-else :items="breadcrumbs" :label="t('current_path')" wrap @navigate="emit('enterDir', $event)" />
      </div>

      <div class="list" :aria-busy="listing || loadingMounts">
        <div v-if="listing || loadingMounts" class="loading-row" role="status">
          <v-circular-progress indeterminate class="sm" />
          <span>{{ t('loading') }}</span>
        </div>
        <div v-else-if="listingError" class="empty-row" role="alert">
          <span>{{ t(listingError) }}</span>
          <v-text-button @click="emit('reload')">{{ t('retry') }}</v-text-button>
        </div>
        <div v-else-if="selectableDirs.length === 0" class="empty-row">{{ t('folder_picker_empty') }}</div>
        <ul v-else class="directory-list">
          <v-list-item
            v-for="d in selectableDirs"
            :key="d.path"
            class="directory-item"
            :title="dirName(d.path)"
            clickable
            role="button"
            tabindex="0"
            @click="emit('enterDir', d.path)"
            @keydown.enter.prevent="emit('enterDir', d.path)"
            @keydown.space.prevent="emit('enterDir', d.path)"
          >
            <template #start><i-material-symbols:folder-rounded width="20" height="20" /></template>
            <template #end><i-material-symbols:chevron-right-rounded width="16" height="16" /></template>
          </v-list-item>
        </ul>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { IStorageMount } from '@/lib/interfaces'
import { getDirectoryBreadcrumbs, type IFile } from '@/lib/file'
import VBreadcrumb from '@/components/base/VBreadcrumb.vue'
import { getStorageVolumeTitle } from '@/lib/volumes'
import { sortMounts, storageCountText } from '@/lib/storage'
import type { DirectoryPickerVolume } from '@/hooks/directory-picker'

const { t } = useI18n()

const props = defineProps<{
  volumes: IStorageMount[]
  volume?: DirectoryPickerVolume
  loadingMounts?: boolean
  activeRoot: string
  currentDir: string
  listing: boolean
  listingError?: string
  dirItems: Pick<IFile, 'path' | 'isSystem'>[]
  dirName: (path: string) => string
}>()

const emit = defineEmits<{
  (e: 'selectRoot', mountPoint: string): void
  (e: 'enterDir', absPath: string): void
  (e: 'reload'): void
}>()

const selectableDirs = computed(() => props.dirItems.filter((directory) => !directory.isSystem))
const sortedVolumes = computed(() => sortMounts(props.volumes))
const breadcrumbs = computed(() => {
  let title: string
  if (props.volume) title = props.volume.name
  else {
    const volume = props.volumes.find((v) => String(v.mountPoint || '') === props.activeRoot)
    if (!volume) return [{ path: props.currentDir, name: props.activeRoot ? props.currentDir : '—' }]
    title = getStorageVolumeTitle(volume, t)
  }
  return getDirectoryBreadcrumbs(props.currentDir, props.activeRoot, title)
})

function mountCount(m: IStorageMount): string {
  return storageCountText(Number(m.freeBytes || 0), Number(m.totalBytes || 0), t)
}
</script>

<style scoped>
.browser {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 304px;
}

.volumes-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
}

.browser-volumes {
  gap: 8px;
  display: flex;
  flex-direction: column;
}

.browser-volumes,
.directory-list {
  margin: 0;
  padding: 0;
  list-style: none;
}

.volume-item,
.directory-item {
  gap: 8px;
  border-radius: var(--pl-shape-m);
}

.volume-item :deep(.body) {
  gap: 4px;
}
.volume-item :deep(.title),
.volume-item :deep(.subtitle) {
  display: block;
  white-space: normal;
  overflow-wrap: anywhere;
}

.browser-sidebar {
  border-radius: 12px;
  max-height: min(280px, 34vh);
  overflow-y: auto;
}

.browser-main {
  border-radius: 16px;
  background: var(--md-sys-color-surface-container-low);
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.selection {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  width: 100%;
  padding: 16px;
}

.list {
  min-height: 192px;
  padding: 8px;
  overflow: auto;
  flex: 1;
}

.loading-row,
.empty-row {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 16px;
  color: var(--md-sys-color-on-surface-variant);
}

@media (min-width: 704px) {
  .browser {
    flex-direction: row;
    align-items: stretch;
  }

  .browser-sidebar {
    width: 200px;
    flex-shrink: 0;
    max-height: none;
  }

  .browser-main {
    flex: 1;
    min-width: 0;
  }
}
</style>
