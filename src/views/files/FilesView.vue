<template>
  <div class="top-app-bar">
    <v-checkbox touch-target="wrapper" :checked="allChecked" :indeterminate="!allChecked && checked" @change="toggleAllChecked" />
    <div class="title">
      <span v-if="bulkDeleting">{{ $t('deleting') }}</span>
      <span v-else-if="selectedIds.length">{{ $t('x_selected', { count: realAllChecked ? total.toLocaleString() : selectedIds.length.toLocaleString() }) }}</span>
      <div v-else ref="breadcrumbEl" class="breadcrumb">
        <template v-for="(item, index) in breadcrumbPaths" :key="item.path">
          <template v-if="index === 0">
            <span v-if="item.path === filter.parent" v-tooltip="getPageStats()">{{ item.name }} ({{ total }})</span>
            <a v-else v-tooltip="getPageStats()" href="#" @click.stop.prevent="navigateToDir(item.path)">{{ item.name }}</a>
          </template>
          <template v-else>
            <span class="sep">/</span>
            <span v-if="item.path === filter.parent">{{ item.name }} ({{ total }})</span>
            <a v-else href="#" @click.stop.prevent="navigateToDir(item.path)">{{ item.name }}</a>
          </template>
        </template>
      </div>
      <template v-if="checked">
        <v-icon-button v-if="!inZip" v-tooltip="$t('copy')" @click.stop="copyItems"><i-material-symbols:content-copy-outline-rounded /></v-icon-button>
        <v-icon-button v-if="!inZip" v-tooltip="$t('cut')" @click.stop="cutItems"><i-material-symbols:content-cut-rounded /></v-icon-button>
        <bulk-delete-button v-if="!inZip" :confirming="confirmingDelete" :count="deleteCount" :loading="bulkDeleting" @click="deleteItems" @confirm="doDeleteItems" @cancel="cancelDeleteItems" />
        <v-icon-button v-tooltip="$t('download')" :loading="downloadLoading" @click.stop="downloadItems"><i-material-symbols:download-rounded /></v-icon-button>
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
    <VirtualList v-if="items.length > 0" class="scroller main-list" :data-key="'id'" :data-sources="items" :estimate-size="80">
      <template #item="{ index, item }">
        <FileListItem
:item="item" :index="index" :selected-ids="selectedIds" :shift-effecting-ids="shiftEffectingIds"
          :should-select="shouldSelect" :is-phone="isPhone" :image-error-ids="imageErrorIds"
          :extension-image-error-ids="extensionImageErrorIds" :can-paste="canPaste()" :in-zip="inZip" :handle-item-click="handleItemClick"
          :handle-mouse-over="handleMouseOver" :toggle-select="toggleSelect" :on-image-error="onImageError"
          :on-extension-image-error="onExtensionImageError" :view-item="viewItem" :click-item="clickItem"
          :deleting="deletingIds.includes(item.id)" :collapsing="collapsingIds.includes(item.id)"
          @download-dir="downloadDir" @download-file="downloadFile" @upload-files="uploadFilesClick"
          @upload-dir="uploadDirClick" @delete-item="deleteItem" @duplicate-item="duplicateItem" @cut-item="cutItem"
          @copy-item="copyItem" @paste-item="pasteItem" @copy-link="copyLinkItem" @rename-item="renameItemClick"
          @add-to-favorites="addToFavoritesClick" />
      </template>
    </VirtualList>
    <NoDataPlaceholder v-if="!loading && items.length === 0" :loading="loading" :permissions="app.permissions" permission="WRITE_EXTERNAL_STORAGE" />
    <input ref="fileInput" style="display: none" type="file" multiple @change="uploadChanged" />
    <input ref="dirFileInput" style="display: none" type="file" multiple webkitdirectory mozdirectory directory @change="dirUploadChanged" />
  </div>
</template>

<script setup lang="ts">
import { computed, inject, nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { useFilesStore } from '@/stores/files'
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
import { useFilesData } from './hooks/useFilesData'
import { useFilesBreadcrumb } from './hooks/useFilesBreadcrumb'
import { useFilesActions } from './hooks/useFilesActions'
import { useFilesNavigation } from './hooks/useFilesNavigation'
import { useFilesEvents } from './hooks/useFilesEvents'

const isPhone = inject('isPhone') as boolean
const { t } = useI18n()
const tempStore = useTempStore()
const { app, urlTokenKey, uploads } = storeToRefs(tempStore)
const { selectedFiles, isCut } = storeToRefs(useFilesStore())
const sortItems = getSortItems()
const sources = ref([])

// --- Data & fetch ---
const { filter, q, items, loading, firstInit, refreshing, sorting, isActive, rootDir, fileSortBy, totalRef, buildQ, applyRouteQuery, fetch, route } = useFilesData()

// --- Selection ---
const { selectedIds, allChecked, realAllChecked, clearSelection, toggleAllChecked, toggleSelect, total, checked, shiftEffectingIds, handleItemClick, handleMouseOver, selectAll, shouldSelect } =
  useSelectable(items)
watch(totalRef, (v) => { total.value = v }, { immediate: true })

// --- Shared hooks ---
const { dropping, fileDragEnter, fileDragLeave, dropFiles } = useDragDropUpload(uploads)
const { createPath, createVariables, createMutation } = useCreateDir(urlTokenKey, items)
const { renameItem, renameDone, renameMutation, renameVariables } = useRename(() => { fetch() })
const { mounts, refetch: refetchMounts } = useMounts()
const { downloadFile, downloadDir, downloadFiles } = useDownload(urlTokenKey)
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
const { breadcrumbPaths, getPageStats } = useFilesBreadcrumb(filter, rootDir, app, mounts, t)
const breadcrumbEl = ref<HTMLElement | null>(null)
watch(breadcrumbPaths, () => {
  nextTick(() => {
    if (breadcrumbEl.value) breadcrumbEl.value.scrollLeft = breadcrumbEl.value.scrollWidth
  })
}, { immediate: true })

// --- Navigation ---
const { navigateToDir, toggleShowHidden, clickItem, viewItem } = useFilesNavigation({
  filter, rootDir, urlTokenKey, buildQ, clearSelection, view, downloadFile, items,
})

// --- Actions ---
const {
  downloadLoading, downloadItems, deleteItems, deleteItem,
  confirmingDelete, deleteCount, doDeleteItems, cancelDeleteItems,
  deletingIds, collapsingIds, bulkDeleting,
  copyItems, cutItems, pasteDir,
  duplicateItem, cutItem, copyItem, pasteItem, copyLinkItem, renameItemClick, createDir,
  addToFavoritesClick, onDeleted,
} = useFilesActions({
  items, total, selectedIds, clearSelection, filter, rootDir, urlTokenKey, t,
  fetch, refetchMounts, copy, cut, paste, downloadFile, downloadDir, downloadFiles,
  createPath, createVariables, createMutation, renameItem, renameMutation, renameVariables, renameDone,
})

// --- Keyboard ---
const { keyDown: pageKeyDown, keyUp: pageKeyUp } = useFilesKeyEvents(selectAll, clearSelection, deleteItems)

// --- Events & lifecycle ---
useFilesEvents({
  isActive, fileSortBy, routeFullPath: () => route.fullPath, applyRouteQuery, fetch, refetchMounts,
  refreshing, sorting, onDeleted, pageKeyDown, pageKeyUp, parentDir: () => filter.parent,
})

// --- Toolbar helpers ---
function sort(value: string) { if (fileSortBy.value !== value) { sorting.value = true; fileSortBy.value = value } }
function refreshCurrentDir() { refreshing.value = true; fetch() }
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
.breadcrumb {
  display: flex;
  align-items: center;
  overflow-x: auto;
  flex-wrap: nowrap;
  scrollbar-width: none; // Firefox
  &::-webkit-scrollbar { display: none; } // Chrome/Safari

  .sep {
    flex-shrink: 0;
    margin-inline: 4px;
  }

  a, span:not(.sep) {
    white-space: nowrap;
    flex-shrink: 0;
  }
}

</style>
