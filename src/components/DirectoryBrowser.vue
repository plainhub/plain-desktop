<template>
  <div class="browser" :style="browserStyle">
    <aside class="browser-sidebar">
      <div v-if="loadingMounts" class="volumes-loading">
        <v-circular-progress indeterminate class="sm" />
      </div>
      <div v-else class="browser-volumes">
        <VolumeCard
          v-for="v in sortedVolumes"
          :key="v.id"
          :title="getStorageVolumeTitle(v, t)"
          :used-percent="mountUsedPercent(v)"
          :count="mountCount(v)"
          :show-progress="Number(v.totalBytes || 0) > 0"
          :data="v"
          :active="String(v.mountPoint || '') === activeRoot"
          @click="emit('selectRoot', v.mountPoint || '/')"
        />
      </div>
    </aside>

    <section class="browser-main">
      <div class="toolbar">
        <v-icon-button v-if="canGoUp" v-tooltip="t('back')" @click.stop="emit('goUp')">
          <i-material-symbols:arrow-back-rounded />
        </v-icon-button>
        <div class="path">{{ currentDir }}</div>
        <slot name="toolbar-actions" />
      </div>

      <div class="list" :class="{ loading: listing }" :style="listStyle">
        <div v-show="listing" class="loading-overlay" aria-live="polite" aria-busy="true">
          <div class="loading-row">
            <v-circular-progress indeterminate class="sm" />
            <span>{{ t('loading') }}</span>
          </div>
        </div>

        <div v-show="!listing && dirItems.length === 0" class="empty-row">{{ t('no_data') }}</div>

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
        </button>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, toRef } from 'vue'
import { useI18n } from 'vue-i18n'
import VolumeCard from '@/components/storage/VolumeCard.vue'
import type { IStorageMount } from '@/lib/interfaces'
import type { IFile } from '@/lib/file'
import SystemFilesHeader from '@/views/files/SystemFilesHeader.vue'
import { useSystemFiles } from '@/views/files/hooks/useSystemFiles'
import { getStorageVolumeTitle } from '@/lib/volumes'
import { sortMounts, storageUsedPercent, storageCountText } from '@/lib/storage'

const { t } = useI18n()

const props = defineProps<{
  volumes: IStorageMount[]
  loadingMounts?: boolean
  activeRoot: string
  currentDir: string
  canGoUp: boolean
  listing: boolean
  dirItems: Pick<IFile, 'path' | 'isSystem'>[]
  dirName: (path: string) => string
  browserMinHeightPx?: number
  listMinHeightPx?: number
}>()

const emit = defineEmits<{
  (e: 'selectRoot', mountPoint: string): void
  (e: 'goUp'): void
  (e: 'enterDir', absPath: string): void
}>()

const { expanded: systemExpanded, visibleItems, systemItems } = useSystemFiles(
  toRef(props, 'dirItems'), () => props.currentDir, () => true,
)

const sortedVolumes = computed(() => sortMounts(props.volumes))
function mountUsedPercent(m: IStorageMount): number {
  return storageUsedPercent(Number(m.freeBytes || 0), Number(m.totalBytes || 0))
}

function mountCount(m: IStorageMount): string {
  return storageCountText(Number(m.freeBytes || 0), Number(m.totalBytes || 0), t)
}

const browserStyle = computed(() => {
  const minH = Number(props.browserMinHeightPx ?? 260)
  return minH ? { minHeight: `${minH}px` } : undefined
})

const listStyle = computed(() => {
  const minH = Number(props.listMinHeightPx ?? 160)
  return minH ? { minHeight: `${minH}px` } : undefined
})
</script>

<style scoped>
.browser {
  display: flex;
  flex-direction: column;
  gap: 12px;
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

.browser-sidebar {
  border-radius: 12px;
  max-height: min(280px, 34vh);
  overflow-y: auto;
}

.browser-main {
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 12px;
  background: var(--md-sys-color-surface-container-low);
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 16px;
  border-bottom: 1px solid var(--md-sys-color-outline-variant);
  background: var(--md-sys-color-surface-container-low);
}

.path {
  flex: 1;
  font-weight: 600;
  color: var(--md-sys-color-on-surface);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.list {
  position: relative;
  padding: 8px;
  overflow: auto;
  flex: 1;
}

.loading-overlay {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in srgb, var(--md-sys-color-surface-container-low) 65%, transparent);
  backdrop-filter: blur(2px);
  pointer-events: none;
}

.list.loading .dir-row {
  pointer-events: none;
}

.loading-row,
.empty-row {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 12px;
  color: var(--md-sys-color-on-surface-variant);
}

.dir-row {
  width: 100%;
  display: grid;
  grid-template-columns: 28px 1fr;
  align-items: center;
  min-height: 42px;
  border: 0;
  background: transparent;
  border-radius: 10px;
  cursor: pointer;
  text-align: left;
  color: var(--md-sys-color-on-surface-variant);
}

.dir-row:hover {
  background: color-mix(in srgb, var(--md-sys-color-primary) 6%, transparent);
}

.dir-icon {
  width: 28px;
  height: 28px;
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

@media (min-width: 980px) {
  .browser {
    flex-direction: row;
    align-items: stretch;
  }

  .browser-sidebar {
    width: 320px;
    max-height: none;
  }

  .browser-main {
    flex: 1;
  }
}
</style>
