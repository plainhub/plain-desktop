<template>
  <div class="browser">
    <aside class="browser-sidebar">
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
          <template #start><i-lucide:hard-drive /></template>
          <template v-if="String(v.mountPoint || '') === activeRoot" #end>
            <i-material-symbols:check-rounded />
          </template>
        </v-list-item>
      </ul>
    </aside>

    <section class="browser-main">
      <div class="toolbar">
        <v-text-button :disabled="!canGoUp" @click="emit('goUp')">
          <i-material-symbols:arrow-back-rounded />
          {{ t('parent_folder') }}
        </v-text-button>
        <div class="selection" aria-live="polite">
          <span class="selection-label">{{ t('folder_picker_current') }}</span>
          <span class="path" :title="currentDir">{{ loadingMounts ? t('loading') : selectionPath }}</span>
        </div>
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
        <div v-else-if="dirItems.length === 0" class="empty-row">{{ t('folder_picker_empty') }}</div>
        <template v-else>
          <SystemFilesHeader
            v-if="systemItems.length"
            :count="systemItems.length"
            :expanded="systemExpanded"
            @toggle="systemExpanded = !systemExpanded"
          />
          <button
            v-for="d in visibleItems"
            :key="d.path"
            class="dir-row"
            @click="emit('enterDir', d.path)"
          >
            <span class="dir-icon">
              <i-material-symbols:folder-rounded />
            </span>
            <span class="dir-name">{{ dirName(d.path) }}</span>
            <i-material-symbols:chevron-right-rounded class="dir-chevron" />
          </button>
        </template>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, toRef } from 'vue'
import { useI18n } from 'vue-i18n'
import type { IStorageMount } from '@/lib/interfaces'
import type { IFile } from '@/lib/file'
import SystemFilesHeader from '@/views/files/SystemFilesHeader.vue'
import { useSystemFiles } from '@/views/files/hooks/useSystemFiles'
import { getStorageVolumeTitle } from '@/lib/volumes'
import { sortMounts, storageCountText } from '@/lib/storage'

const { t } = useI18n()

const props = defineProps<{
  volumes: IStorageMount[]
  loadingMounts?: boolean
  activeRoot: string
  currentDir: string
  canGoUp: boolean
  listing: boolean
  listingError?: string
  dirItems: Pick<IFile, 'path' | 'isSystem'>[]
  dirName: (path: string) => string
}>()

const emit = defineEmits<{
  (e: 'selectRoot', mountPoint: string): void
  (e: 'goUp'): void
  (e: 'enterDir', absPath: string): void
  (e: 'reload'): void
}>()

const { expanded: systemExpanded, visibleItems, systemItems } = useSystemFiles(
  toRef(props, 'dirItems'), () => props.currentDir, () => true,
)

const sortedVolumes = computed(() => sortMounts(props.volumes))
const selectionPath = computed(() => {
  const volume = props.volumes.find((v) => String(v.mountPoint || '') === props.activeRoot)
  if (!volume) return props.activeRoot ? props.currentDir : '—'
  const title = getStorageVolumeTitle(volume, t)
  const relative = props.currentDir.slice(props.activeRoot.length).replace(/^\/+/, '')
  return relative ? `${title} / ${relative}` : title
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
  margin: 0;
  padding: 0;
  list-style: none;
}

.volume-item {
  gap: 8px;
  border-radius: 12px;
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

.toolbar {
  display: flex;
  align-items: flex-start;
  flex-direction: column;
  gap: 8px;
  padding: 8px 16px 16px;
}

.selection {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  width: 100%;
}

.selection-label {
  font-size: 0.75rem;
  color: var(--md-sys-color-on-surface-variant);
}

.path {
  font-weight: 500;
  color: var(--md-sys-color-on-surface);
  overflow-wrap: anywhere;
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

.dir-row {
  width: 100%;
  display: grid;
  grid-template-columns: 24px minmax(0, 1fr) 24px;
  align-items: center;
  min-height: 48px;
  gap: 8px;
  padding: 8px;
  border: 0;
  background: transparent;
  border-radius: 8px;
  cursor: pointer;
  text-align: left;
  color: var(--md-sys-color-on-surface-variant);
}

.dir-row:hover {
  background: color-mix(in srgb, var(--md-sys-color-primary) 6%, transparent);
}

.dir-icon {
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.dir-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--md-sys-color-on-surface);
}

.dir-chevron {
  width: 24px;
  height: 24px;
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
