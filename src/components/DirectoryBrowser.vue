<template>
  <div class="browser">
    <aside v-if="!volume || recentItems.length" class="browser-sidebar">
      <div v-if="!volume && loadingMounts" class="volumes-loading">
        <v-circular-progress indeterminate class="sm" />
      </div>
      <ul v-else-if="!volume" class="browser-volumes" :aria-label="t('volumes')">
        <v-list-item
          v-for="v in sortedVolumes"
          :key="v.id"
          class="volume-item"
          role="button"
          tabindex="0"
          clickable
          :title="volumeTitle(v)"
          :subtitle="mountCount(v)"
          :active="directoryPathKey(v.mountPoint) === directoryPathKey(activeRoot)"
          :aria-pressed="directoryPathKey(v.mountPoint) === directoryPathKey(activeRoot)"
          @click="emit('selectRoot', v.mountPoint || '/')"
          @keydown.enter.prevent="emit('selectRoot', v.mountPoint || '/')"
          @keydown.space.prevent="emit('selectRoot', v.mountPoint || '/')"
        >
          <template #start><i-lucide:hard-drive width="20" height="20" /></template>
          <template v-if="directoryPathKey(v.mountPoint) === directoryPathKey(activeRoot)" #end>
            <i-material-symbols:check-rounded width="16" height="16" />
          </template>
        </v-list-item>
      </ul>
      <section v-if="recentItems.length" class="browser-recents">
        <div class="subtle recent-heading">{{ t('recent_folders') }}</div>
        <ul class="browser-volumes" :aria-label="t('recent_folders')">
          <v-list-item
            v-for="item in recentItems"
            :key="item.path"
            class="recent-item"
            role="button"
            tabindex="0"
            clickable
            :title="item.name"
            :subtitle="item.subtitle"
            :active="directoryPathKey(item.path) === directoryPathKey(currentDir)"
            :aria-disabled="loadingMounts || undefined"
            @click="selectRecent(item.path)"
            @keydown.enter.prevent="selectRecent(item.path)"
            @keydown.space.prevent="selectRecent(item.path)"
          >
            <template #start><i-material-symbols:folder-outline-rounded width="20" height="20" /></template>
          </v-list-item>
        </ul>
      </section>
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
import { sortMounts } from '@/lib/storage'
import { formatFileSize } from '@/lib/format'
import { directoryPathKey, directoryWithinRoot, findDirectoryVolume, normalizeDirectoryPath } from '@/lib/directory-picker-path'
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
  recentDirs: string[]
  dirName: (path: string) => string
}>()

const emit = defineEmits<{
  (e: 'selectRoot', mountPoint: string): void
  (e: 'selectRecent', path: string): void
  (e: 'enterDir', absPath: string): void
  (e: 'reload'): void
}>()

const selectableDirs = computed(() => props.dirItems.filter((directory) => !directory.isSystem))
const sortedVolumes = computed(() => sortMounts(props.volumes))
const recentItems = computed(() => props.recentDirs.flatMap(path => {
  const normalized = normalizeDirectoryPath(path)
  if (props.volume && !directoryWithinRoot(normalized, props.volume.mountPoint)) return []
  const lastSlash = normalized.lastIndexOf('/')
  const drive = /^[a-z]:\//i.test(normalized)
  const mount = findDirectoryVolume(normalized, props.volumes)
  const volume = props.volume ?? mount
  const title = props.volume ? props.volume.name : mount ? volumeTitle(mount) : ''
  const subtitle = volume
    ? getDirectoryBreadcrumbs(normalized, volume.mountPoint, title).map(item => item.name).join('/')
    : drive ? normalized.replaceAll('/', '\\') : normalized
  return [{ path: normalized, name: normalized.slice(lastSlash + 1) || normalized, subtitle }]
}))

function selectRecent(path: string) {
  if (!props.loadingMounts) emit('selectRecent', path)
}

function volumeTitle(volume: IStorageMount) {
  const title = getStorageVolumeTitle(volume, t)
  const root = normalizeDirectoryPath(volume.mountPoint)
  if (!/^[a-z]:\/$/i.test(root)) return title
  const drive = root.slice(0, 2)
  if (title.toUpperCase() === drive || directoryPathKey(title) === directoryPathKey(root)) return drive
  return `${title} (${drive})`
}

const breadcrumbs = computed(() => {
  let title: string
  if (props.volume) title = props.volume.name
  else {
    const volume = props.volumes.find((v) => directoryPathKey(v.mountPoint) === directoryPathKey(props.activeRoot))
    if (!volume) return [{ path: props.currentDir, name: props.activeRoot ? props.currentDir : '—' }]
    title = volumeTitle(volume)
  }
  return getDirectoryBreadcrumbs(props.currentDir, props.activeRoot, title)
})

function mountCount(m: IStorageMount): string {
  const total = Number(m.totalBytes || 0)
  if (total <= 0) return ''
  const size = (bytes: number) => formatFileSize(bytes, true, bytes >= 100e9 && bytes < 1e12 ? 0 : 1)
  return t('storage_free_compact', { free: size(Number(m.freeBytes || 0)), total: size(total) })
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
.recent-item,
.directory-item {
  gap: 8px;
  border-radius: var(--pl-shape-m);
}

.volume-item {
  padding-inline: 8px;
}

.volume-item :deep(.body) {
  gap: 4px;
}
.browser-recents {
  margin-top: 16px;
}

.browser-recents:first-child {
  margin-top: 0;
}

.recent-heading {
  padding: 0 16px 8px;
}

.volume-item :deep(.subtitle) {
  font-variant-numeric: tabular-nums;
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
    width: 224px;
    flex-shrink: 0;
    max-height: min(480px, 58vh);
  }

  .browser-main {
    flex: 1;
    min-width: 0;
  }
}
</style>
