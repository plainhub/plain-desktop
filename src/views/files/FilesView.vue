<template>
  <div class="top-app-bar">
    <v-checkbox touch-target="wrapper" :checked="allChecked" :indeterminate="!allChecked && checked" @change="toggleAllChecked" />
    <div class="title">
      <span v-if="bulkDeleting">{{ $t('deleting') }}</span>
      <span v-else-if="selectedIds.length">{{ $t('x_selected', { count: realAllChecked ? total.toLocaleString() : selectedIds.length.toLocaleString() }) }}</span>
      <VBreadcrumb v-else :items="breadcrumbPaths" :label="$t('current_path')" :tooltip="(item, index) => index === 0 || item.path === filter.parent ? pageDetails : ''" @navigate="navigateToDir">
        <template v-if="indexed && filter.parent === rootDir" #name>{{ $t('indexed_files') }}</template>
        <template #suffix="{ current }">
          <template v-if="current"> ({{ regularTotal }})<span v-if="searchState" class="search-state"> · {{ searchState }}</span></template>
        </template>
      </VBreadcrumb>
      <template v-if="checked">
        <v-icon-button v-if="!inZip" v-tooltip="$t('copy')" @click.stop="copyItems"><i-material-symbols:content-copy-outline-rounded /></v-icon-button>
        <v-icon-button v-if="!inZip" v-tooltip="$t('cut')" @click.stop="cutItems"><i-material-symbols:content-cut-rounded /></v-icon-button>
        <bulk-delete-button v-if="!inZip" :confirming="confirmingDelete" :count="deleteCount" :loading="bulkDeleting" @click="deleteItems" @confirm="doDeleteItems" @cancel="cancelDeleteItems" />
        <bulk-download-button :single="selectedIds.length === 1" :loading="downloadLoading" @download="downloadSelectedItem" @download-each="downloadEachItems" @download-zip="downloadZipItems" />
      </template>
    </div>
    <div v-if="!isPhone && !checked" class="actions">
      <FilesActionButtons v-bind="actionButtonProps" />
    </div>
  </div>

  <div v-if="isPhone && !checked" class="secondary-actions">
    <FilesActionButtons v-bind="actionButtonProps" />
  </div>

  <div v-if="loading && firstInit" class="scroller-wrapper">
    <div class="scroller main-list">
      <FileSkeletonItem v-for="i in 20" :key="i" :index="i" :is-phone="isPhone" />
    </div>
  </div>
  <div class="scroller-wrapper" @dragover.stop.prevent="fileDragEnter">
    <div v-show="dropping" class="drag-mask" @drop.stop.prevent="dropFiles2" @dragleave.stop.prevent="fileDragLeave">{{ $t('release_to_send_files') }}</div>
    <VirtualList v-if="items.length > 0" class="scroller main-list" :class="{ 'system-files-list': systemItems.length > 0 }" :data-key="'id'" :data-sources="visibleItems" :estimate-size="80" :keeps="renderedRows">
      <template v-if="systemItems.length" #header>
        <SystemFilesHeader :count="systemItems.length" :expanded="systemExpanded" @toggle="systemExpanded = !systemExpanded" />
      </template>
      <template #item="{ index, item }">
        <div :class="{ 'system-file-row': systemItems.length && item.isSystem, 'system-file-last': systemItems.length && index === systemItems.length - 1 }">
        <FileListItem
:item="item" :index="index" :selected-ids="selectedIds" :shift-effecting-ids="shiftEffectingIds"
          :should-select="shouldSelect" :is-phone="isPhone" :image-error-ids="imageErrorIds"
          :extension-image-error-ids="extensionImageErrorIds" :can-paste="canPaste()" :in-zip="inZip" :handle-item-click="handleItemClick"
          :handle-mouse-over="handleMouseOver" :toggle-select="toggleSelect" :on-image-error="onImageError"
          :on-extension-image-error="onExtensionImageError" :view-item="viewItem" :click-item="clickItem"
          :deleting="deletingIds.includes(item.id)" :collapsing="collapsingIds.includes(item.id)"
          @download-dir="downloadDir" @download-file="downloadFile" @save-as="downloadFileAs" @upload-files="uploadFilesClick"
          @upload-dir="uploadDirClick" @delete-item="deleteItem" @duplicate-item="duplicateItem" @cut-item="cutItem"
          @copy-item="copyItem" @paste-item="pasteItem" @copy-link="copyLinkItem" @rename-item="renameItemClick"
          @add-to-favorites="addToFavoritesClick" />
        </div>
      </template>
    </VirtualList>
    <NoDataPlaceholder v-if="!loading && items.length === 0" :loading="loading" :access="app.access" feature="FILES" />
    <input ref="fileInput" style="display: none" type="file" multiple @change="uploadChanged" />
    <input ref="dirFileInput" style="display: none" type="file" multiple webkitdirectory mozdirectory directory @change="dirUploadChanged" />
  </div>
</template>

<script setup lang="ts">
import { computed, inject, ref, watch } from 'vue'
import VBreadcrumb from '@/components/base/VBreadcrumb.vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { useFilesStore } from '@/stores/files'
import { useMainStore } from '@/stores/main'
import { getSortItems, isZipPath } from '@/lib/file'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import { useCreateDir, useRename, useMounts, useDownload, useView, useCopyPaste } from '@/hooks/files'
import { useDragDropUpload, useFileUpload } from '@/hooks/upload'
import { useSelectable } from '@/hooks/list'
import { useFilesKeyEvents } from '@/hooks/key-events'
import { openModal } from '@/components/modal'
import { useOpenMedia } from '@/hooks/open-media'
import KeyboardShortcutsModal from '@/components/KeyboardShortcutsModal.vue'
import { filesKeyboardShortcuts } from '@/lib/shortcuts/files'
import type { ISource } from '@/components/lightbox/types'
import VirtualList from '@/components/virtualscroll'
import { useVirtualRowCount } from '@/hooks/device'
import { useFilesData } from './hooks/useFilesData'
import { useFilesBreadcrumb } from './hooks/useFilesBreadcrumb'
import { useFilesActions } from './hooks/useFilesActions'
import { useFilesNavigation } from './hooks/useFilesNavigation'
import { useFilesEvents } from './hooks/useFilesEvents'
import { useFileIndexStatus } from './hooks/useFileIndexStatus'
import { useSystemFiles } from './hooks/useSystemFiles'
import SystemFilesHeader from './SystemFilesHeader.vue'

const isPhone = inject('isPhone') as boolean
const renderedRows = useVirtualRowCount(80)
const { t } = useI18n()
const tempStore = useTempStore()
const { app, urlTokenKey, uploads } = storeToRefs(tempStore)
const { selectedFiles, isCut } = storeToRefs(useFilesStore())
const { systemFilesExpanded } = storeToRefs(useMainStore())
const sortItems = getSortItems()
const sources = ref([])

// --- Data & fetch ---
const { filter, q, items, loading, firstInit, refreshing, sorting, isActive, rootDir, indexed, fileSortBy, totalRef, buildQ, applyRouteQuery, fetch, route, cancel, searchStatus } = useFilesData()

const { expanded: systemExpanded, visibleItems, systemItems } = useSystemFiles(
  items, () => filter.parent,
  () => !indexed.value && !filter.text && !q.value.includes('file_size:') && !filter.parent.includes('!zip!/'),
  systemFilesExpanded,
)
const regularTotal = computed(() => Math.max(0, totalRef.value - systemItems.value.length))

// --- Selection ---
const { selectedIds, allChecked, realAllChecked, clearSelection, toggleAllChecked, toggleSelect, total, checked, shiftEffectingIds, handleItemClick, handleMouseOver, selectAll, shouldSelect } =
  useSelectable(visibleItems)
watch(() => systemItems.value.length ? visibleItems.value.length : totalRef.value, (v) => { total.value = v }, { immediate: true })
watch(systemExpanded, clearSelection)

// --- Shared hooks ---
const { dropping, fileDragEnter, fileDragLeave, dropFiles } = useDragDropUpload(uploads)
const { createPath, createVariables, createMutation } = useCreateDir(urlTokenKey, items)
const { renameItem, renameDone, renameMutation, renameVariables } = useRename(() => { fetch() })
const { mounts, refetch: refetchMounts } = useMounts()
const { downloadFile, downloadFileAs, downloadDir, downloadFiles } = useDownload(urlTokenKey)
const { open: openMedia } = useOpenMedia()
const { view } = useView(sources, (s: ISource[], index: number) => {
  const target = s[index]
  if (!target) return
  openMedia(index, s)
})
const { loading: pasting, canPaste, copy, cut, paste } = useCopyPaste(items, isCut, selectedFiles, fetch, refetchMounts)
const { input: fileInput, upload: uploadFiles, uploadChanged } = useFileUpload(uploads)
const { input: dirFileInput, upload: uploadDir, uploadChanged: dirUploadChanged } = useFileUpload(uploads)

const imageErrorIds = ref<string[]>([])
const extensionImageErrorIds = ref<string[]>([])
const onImageError = (id: string) => { imageErrorIds.value.push(id) }
const onExtensionImageError = (id: string) => { extensionImageErrorIds.value.push(id) }

// --- Breadcrumb ---
const { breadcrumbPaths, getPageStats } = useFilesBreadcrumb(filter, rootDir, mounts, t)
const { progress: indexProgress, error: indexError } = useFileIndexStatus(computed(() => indexed.value && isActive.value))
const searchState = computed(() => {
  if (!indexed.value) return searchStatus.value === 'idle' ? '' : t(`directory_search_${searchStatus.value}`)
  if (indexError.value) return t('index_state_failed')
  const state = indexProgress.value?.state
  const status = { IDLE: 'completed', RUNNING: 'running', PAUSED: 'paused', STOPPED: 'canceled', FAILED: 'failed' } as const
  return t(`index_state_${state ? status[state] : 'pending'}`)
})
const pageDetails = computed(() => {
  const details = [getPageStats()]
  if (indexed.value) {
    details.push(t('index_search_scope_hint'))
    if (indexError.value || indexProgress.value?.error) details.push(indexError.value || indexProgress.value?.error || '')
  } else if (searchStatus.value !== 'idle') details.push(t('directory_search_scope_hint'))
  return details.filter(Boolean).join('\n')
})

// --- Navigation ---
const { navigateToDir, toggleShowHidden, clickItem, viewItem } = useFilesNavigation({
  filter, rootDir, urlTokenKey, buildQ, clearSelection, view, downloadFile, items: visibleItems,
})

// --- Actions ---
const {
  downloadLoading, downloadSelectedItem, downloadEachItems, downloadZipItems, deleteItems, deleteItem,
  confirmingDelete, deleteCount, doDeleteItems, cancelDeleteItems,
  deletingIds, collapsingIds, bulkDeleting,
  copyItems, cutItems, pasteDir,
  duplicateItem, cutItem, copyItem, pasteItem, copyLinkItem, renameItemClick, createDir,
  addToFavoritesClick, onDeleted,
} = useFilesActions({
  items, total: totalRef, selectedIds, clearSelection, filter, rootDir, urlTokenKey, t,
  fetch, refetchMounts, copy, cut, paste, downloadFile, downloadDir, downloadFiles,
  createPath, createVariables, createMutation, renameItem, renameMutation, renameVariables, renameDone,
})

// --- Keyboard ---
const { keyDown: pageKeyDown, keyUp: pageKeyUp } = useFilesKeyEvents(selectAll, clearSelection, deleteItems)

// --- Events & lifecycle ---
useFilesEvents({
  isActive, fileSortBy, routeFullPath: () => route.path === '/files' ? route.fullPath : null, applyRouteQuery, fetch, refetchMounts,
  refreshing, sorting, onDeleted, pageKeyDown, pageKeyUp, parentDir: () => filter.parent,
})

// --- Toolbar helpers ---
function sort(value: string) { if (fileSortBy.value !== value) { sorting.value = true; fileSortBy.value = value } }
function refreshCurrentDir() {
  if (searchStatus.value === 'running') { cancel(); return }
  refreshing.value = true; fetch()
}
function openKeyboardShortcuts() { openModal(KeyboardShortcutsModal, { title: t('keyboard_shortcuts'), shortcuts: filesKeyboardShortcuts }) }
function uploadFilesClick(dir: string) { uploadFiles(dir) }
function uploadDirClick(dir: string) { uploadDir(dir) }
function dropFiles2(e: DragEvent) { dropFiles(e, filter.parent, () => true) }

const inZip = computed(() => isZipPath(filter.parent))

const actionButtonProps = computed(() => ({
  currentDir: filter.parent,
  inZip: inZip.value,
  canPaste: canPaste(),
  pasting: pasting.value,
  refreshing: refreshing.value,
  searching: searchStatus.value === 'running',
  sorting: sorting.value,
  sortItems,
  fileSortBy: fileSortBy.value,
  showHidden: filter.showHidden,
  onCreateDir: createDir,
  onUploadFiles: uploadFilesClick,
  onUploadDir: uploadDirClick,
  onPasteDir: pasteDir,
  onRefreshCurrentDir: refreshCurrentDir,
  onSort: sort,
  onOpenKeyboardShortcuts: openKeyboardShortcuts,
  onToggleShowHidden: toggleShowHidden,
}))
</script>

<style lang="scss" scoped>
.search-state { color: var(--md-sys-color-on-surface-variant); font-size: 0.875rem; font-weight: 400; }

</style>

<style scoped>
.system-files-list {
  gap: 0;
}
.system-file-row {
  margin-inline: 16px;
  background: var(--md-sys-color-surface-container);
}
.system-file-row :deep(.selectable-card),
.system-file-row :deep(.list-item-phone) {
  margin-inline: 0;
  margin-bottom: 0;
  border-radius: 0;
}
.system-file-last {
  border-radius: 0 0 8px 8px;
  margin-bottom: 8px;
  overflow: hidden;
}
</style>
