<template>
  <MediaToolbar
    page-title="page_title.docs"
    :selected-count="selectedIds.length"
    :all-checked="allChecked" :checked="checked" :real-all-checked="realAllChecked" :total="total"
    :filter-trash="!!filter.trash"
    :can-trash="hasFeature(FEATURE.MEDIA_TRASH, app.osVersion)"
    :restore-query-loading="restoreLoading(getQuery())" :trash-query-loading="trashLoading(getQuery())"
    :limit="limit" :all-checked-alert-visible="allCheckedAlertVisible"
    :show-secondary="false"
    :confirming-delete="confirmingDelete" :delete-count="deleteCount" :delete-loading="deleteLoading"
    @toggle-all-checked="toggleAllChecked" @delete="deleteItems(dataType, selectedIds, realAllChecked, total, q)"
    @confirm="doDeleteItems" @cancel="cancelDeleteItems"
    @restore="restore(dataType, getQuery())" @download="downloadItems(realAllChecked, selectedIds, q)"
    @trash="trash(dataType, getQuery())"
    @select-real-all="selectRealAll" @clear-selection="clearSelection"
  >
    <template #tag-action>
      <BulkTagDropdown :type="dataType" :tags="tags" :items="items" :selected-ids="selectedIds" :real-all-checked="realAllChecked" :q="q" />
    </template>
    <template #extra-actions>
      <v-icon-button v-if="!filter.trash" v-tooltip="$t('move_to_folder')" :loading="moveLoading" @click.stop="onMoveClick(dataType, selectedIds, realAllChecked, getQuery())">
        <i-material-symbols:drive-file-move-outline-rounded />
      </v-icon-button>
    </template>
    <template #actions>
      <MediaPageActions v-bind="actionsProps" placement="top" />
    </template>
  </MediaToolbar>

  <div class="scroll-content" @dragover.stop.prevent="fileDragEnter">
    <div v-show="dropping" class="drag-mask" @drop.stop.prevent="dropFiles2" @dragleave.stop.prevent="fileDragLeave">{{ $t('release_to_send_files') }}</div>
    <div class="main-list" :class="{ 'select-mode': checked }">
      <template v-if="loading && items.length === 0">
        <DocSkeletonItem v-for="i in 20" :key="i" :index="i" />
      </template>
      <DocListItem
        v-for="(item, i) in items"
        :key="item.id"
        :item="item"
        :index="i"
        :selected-ids="selectedIds"
        :shift-effecting-ids="shiftEffectingIds"
        :should-select="shouldSelect"
        :filter="filter"
        :app="app"
        :data-type="dataType"
        :trash-loading="trashLoading"
        :restore-loading="restoreLoading"
        :trash="trash"
        :restore="restore"
        :delete-item="deleteDocItemInTrash"
        :buckets-map="bucketsMap"
        :view-bucket="viewBucket"
        :tags="tags"
        :handle-item-click="handleItemClick"
        :handle-mouse-over="handleMouseOverMode"
        :toggle-select="toggleSelect"
        @download-file="downloadFile"
        @delete-item="deleteDocItem"
        @open-file="openFile"
        @rename-item="renameItem"
        @duplicate-item="duplicateItem"
      />
    </div>
    <NoDataPlaceholder v-if="!loading && items.length === 0" :loading="loading" :permissions="app.permissions" permission="WRITE_EXTERNAL_STORAGE" />
    <v-pagination v-if="!scrollMode && total > limit" :page="page" :go="gotoPage" :total="total" :limit="limit" :page-size="limit" :on-change-page-size="onChangePageSize" />
    <div v-if="scrollMode" ref="sentinel" class="scroll-sentinel"></div>
    <input ref="fileInput" style="display: none" type="file" multiple accept=".doc,.docx,.xls,.xlsx,.ppt,.pptx,.pdf,.txt,.md,.csv,.json,.xml,.js,.ts,.py,.java,.kt,.swift,.c,.cpp,.h,.cs,.go,.rs,.rb,.sh,.yaml,.yml,.toml,.ini,.cfg,.log" @change="uploadChanged" />
    <input ref="dirFileInput" style="display: none" type="file" multiple webkitdirectory mozdirectory directory @change="dirUploadChanged" />
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import toast from '@/components/toaster'
import { docsGQL, initLazyQuery } from '@/lib/api/query'
import { deleteMediaItemsGQL, initMutation } from '@/lib/api/mutation'
import type { IDoc } from '@/lib/interfaces'
import { DataType, FEATURE } from '@/lib/data'
import { getSortItems, isDoc } from '@/lib/file'
import { hasFeature } from '@/lib/feature'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { useMediaPage } from '@/hooks/media-page'
import { useMediaPageActions } from '@/hooks/media-page-actions'
import { useMoveToFolder } from '@/hooks/media'
import emitter from '@/plugins/eventbus'
import MediaPageActions from '@/components/media/MediaPageActions.vue'
import MediaToolbar from '@/components/media/MediaToolbar.vue'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import DocListItem from './DocListItem.vue'
import DocSkeletonItem from './DocSkeletonItem.vue'
import { useDocsActions } from './hooks/useDocsActions'

const mainStoreLocal = useMainStore()
const { docSortBy, docsScrollPaging } = storeToRefs(mainStoreLocal)
const items = ref<IDoc[]>([])
const sortItems = getSortItems()

const scrollMode = computed(() => mainStoreLocal.docsScrollPaging)
const noMore = ref(false)
const sentinel = ref<HTMLElement | null>(null)

let observer: IntersectionObserver | null = null
function setupSentinelObserver() {
  if (observer) { observer.disconnect(); observer = null }
  if (!sentinel.value) return
  observer = new IntersectionObserver((entries) => { if (entries[0]?.isIntersecting) loadMore() }, { rootMargin: '200px' })
  observer.observe(sentinel.value)
}
function loadMore() {
  if (noMore.value || loading.value || !scrollMode.value) return
  mp.page.value++
  fetch()
}

watch(scrollMode, (val) => {
  mp.page.value = 1; items.value = []; noMore.value = false
  if (val) setTimeout(setupSentinelObserver, 100)
  else { observer?.disconnect(); observer = null }
  fetch()
})
watch(sentinel, (el) => { if (el && scrollMode.value) setupSentinelObserver() })

const mp = useMediaPage({
  dataType: DataType.DOC, routePath: 'docs',
  items, sortByRef: docSortBy, fileFilter: isDoc,
  downloadName: 'docs', uploadModalId: 'upload-directory-picker-docs', uploadStorageKey: 'docs',
  doFetch: () => fetch(), getScrollMode: () => scrollMode.value,
  setupScroll: () => setTimeout(setupSentinelObserver, 100),
  teardownScroll: () => { observer?.disconnect(); observer = null },
  onSort: () => { noMore.value = false },
})
const {
  isPhone, mainStore, app, urlTokenKey,
  filter, page, q, limit, dataType,
  fileInput, dirFileInput, uploadChanged, dirUploadChanged, dropping, fileDragEnter, fileDragLeave,
  deleteItems, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems, bucketsMap, viewBucket, tags,
  selectedIds, allChecked, realAllChecked, selectRealAll, allCheckedAlertVisible,
  clearSelection, toggleAllChecked, toggleSelect, total, checked, shiftEffectingIds, handleItemClick, shouldSelect,
  downloadItems, downloadFile, trashLoading, trash, restoreLoading, restore,
  gotoPage, onChangePageSize, getQuery, sort, handleMouseOverMode,
  uploadFilesClick, uploadDirClick, dropFiles2,
  uploadDir, uploadDirEditable, editUploadDir,
} = mp

const { mutate: doDeleteDocItem, onDone: onDeleteDocItemDone } = initMutation({ document: deleteMediaItemsGQL })
let pendingDocDeleteEmit: (() => void) | null = null
onDeleteDocItemDone(() => { pendingDocDeleteEmit?.(); pendingDocDeleteEmit = null })

function deleteDocItemInTrash(dt: DataType, item: IDoc) {
  pendingDocDeleteEmit = () => emitter.emit('media_items_actioned', { type: dt, action: 'delete', id: item.id, query: `ids:${item.id}` })
  doDeleteDocItem({ type: dt, query: `ids:${item.id}` })
}

const { loading, fetch } = initLazyQuery({
  handle: (data: { items: IDoc[]; total: number }, error: string) => {
    mp.sorting.value = false
    if (error) {
      toast(error, 'error')
    } else if (data) {
      const raw = data.items
      if (scrollMode.value && page.value > 1) { items.value = items.value.concat(raw) } else { items.value = raw }
      total.value = data.total
      if (scrollMode.value) { noMore.value = data.items.length < limit.value }
    }
  },
  document: docsGQL,
  variables: () => ({ offset: (page.value - 1) * limit.value, limit: limit.value, query: q.value, sortBy: docSortBy.value }),
})

const { openFile, deleteItem: deleteDocItem, renameItem, duplicateItem } = useDocsActions(items, selectedIds, clearSelection, fetch, urlTokenKey)

const { moveLoading, onMoveClick } = useMoveToFolder('move-docs-picker')

const actionsProps = useMediaPageActions({
  filterTrash: computed(() => !!filter.trash),
  checked,
  sortBy: docSortBy,
  sortItems,
  onSort: sort,
  upload: { dir: uploadDir, editable: uploadDirEditable, onUploadFiles: uploadFilesClick, onUploadDir: uploadDirClick, onEditDir: editUploadDir },
  options: {
    show: true,
    scrollPaging: docsScrollPaging,
    onUpdateScrollPaging: (v: boolean) => { docsScrollPaging.value = v },
  },
})
</script>

<style scoped lang="scss">
:deep(.doc-item) {
  grid-template-areas:
    'start icon title actions'
    'start icon subtitle actions';
  grid-template-columns: 48px 40px 1fr auto;
  align-items: center;

  &:hover { cursor: pointer; }

  .doc-icon {
    grid-area: icon;
    display: flex;
    align-items: center;
    margin-block: 10px 8px;
    .svg { width: 32px; height: 32px; object-fit: contain; }
  }

  .title {
    grid-area: title;
    padding-block-start: 8px;
  }

  .subtitle {
    grid-area: subtitle;
    display: flex;
    gap: 12px;
    color: var(--md-sys-color-secondary);
    font-size: 0.85rem;
    margin-block-end: 8px;
  }

  .actions { grid-area: actions; }
}
</style>
