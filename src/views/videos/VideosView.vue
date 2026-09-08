<template>
  <MediaToolbar
    page-title="page_title.videos"
    :selected-count="selectedIds.length"
    :all-checked="allChecked" :checked="checked" :real-all-checked="realAllChecked" :total="total"
    :filter-trash="!!filter.trash"
    :can-trash="hasFeature(FEATURE.MEDIA_TRASH, app.osVersion)"
    :restore-query-loading="restoreLoading(getQuery())" :trash-query-loading="trashLoading(getQuery())"
    :limit="limit" :all-checked-alert-visible="allCheckedAlertVisible"
    :show-secondary="isPhone && !checked"
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
    <template #actions>
      <MediaPageActions v-bind="actionsProps" placement="top" />
    </template>
    <template #secondary>
      <MediaPageActions v-bind="actionsProps" placement="secondary" />
    </template>
  </MediaToolbar>

  <div class="scroll-content" @dragover.stop.prevent="fileDragEnter">
    <div v-show="dropping" class="drag-mask" @drop.stop.prevent="dropFiles2" @dragleave.stop.prevent="fileDragLeave">{{ $t('release_to_send_files') }}</div>

    <template v-if="effectiveIsGroupMode">
      <template v-if="loading && items.length === 0">
        <div class="media-grid"><section v-for="i in limit" :key="i" class="skeleton-image media-item"></section></div>
      </template>
      <div v-for="group in groupedItems" :key="group.date" class="image-group">
        <div class="group-date-label">
          <span class="group-date-text" @click="toggleGroupByText(group)">{{ group.dateLabel }}</span>
          <v-checkbox
            v-if="checked"
            touch-target="wrapper"
            :checked="groupCheckboxState(group).checked"
            :indeterminate="groupCheckboxState(group).indeterminate"
            @change="toggleGroup($event, group)"
          />
        </div>
        <div class="media-grid" :class="{ 'select-mode': checked }">
          <MediaGridItem
v-for="{ item, idx } in group.entries" :key="item.id" :item="item" :checked="checked"
            :selected-ids="selectedIds" :shift-effecting-ids="shiftEffectingIds" :should-select="shouldSelect" :data-type="dataType"
            @item-click="handleItemClick($event, item, idx, view)" @item-mouse-enter="handleMouseOverMode($event, idx)"
            @toggle-select="toggleSelect($event, item, idx)" @view="view(idx)">
            <template #thumbnail>
              <img v-if="imageErrorIds.includes(item.id)" :src="`/ficons/${getFileExtension(item.path)}.svg`" class="image svg" />
              <img v-else class="image image-thumb" :src="getFileUrl(item.fileId, '&w=512&h=512')" loading="lazy" @error="onImageError(item.id)" />
            </template>
            <template #info-right>{{ formatSeconds(item.duration) }}</template>
          </MediaGridItem>
        </div>
      </div>
    </template>

    <div v-else-if="!mainStore.videosCardView" class="media-grid" :class="{ 'select-mode': checked }">
      <MediaGridItem
v-for="(item, i) in items" :key="item.id" :item="item" :checked="checked"
        :selected-ids="selectedIds" :shift-effecting-ids="shiftEffectingIds" :should-select="shouldSelect" :data-type="dataType"
        @item-click="handleItemClick($event, item, i, view)" @item-mouse-enter="handleMouseOverMode($event, i)"
        @toggle-select="toggleSelect($event, item, i)" @view="view(i)">
        <template #thumbnail>
          <img v-if="imageErrorIds.includes(item.id)" :src="`/ficons/${getFileExtension(item.path)}.svg`" class="image svg" />
          <img v-else class="image image-thumb" :src="getFileUrl(item.fileId, '&w=512&h=512')" loading="lazy" @error="onImageError(item.id)" />
        </template>
        <template #info-right>{{ ['SIZE_ASC', 'SIZE_DESC'].includes(videoSortBy) ? formatFileSize(item.size) : formatSeconds(item.duration) }}</template>
      </MediaGridItem>
      <template v-if="loading && items.length === 0"><section v-for="i in limit" :key="i" class="skeleton-image media-item"></section></template>
    </div>

    <div v-else class="main-list media-list" :class="{ 'select-mode': checked }">
      <VideoListItem
v-for="(item, i) in items" :key="item.id" :item="item" :index="i" :is-phone="isPhone"
        :selected-ids="selectedIds" :shift-effecting-ids="shiftEffectingIds" :should-select="shouldSelect"
        :image-error-ids="imageErrorIds" :buckets-map="bucketsMap" :filter="filter" :data-type="dataType" :tags="tags"
        :main-store="mainStore" :app="app" :handle-item-click="handleItemClick" :handle-mouse-over="handleMouseOverMode"
        :toggle-select="toggleSelect" :on-image-error="onImageError" :view-bucket="viewBucket"
        :delete-item="deleteItem" :restore="restore" :download-file="downloadFile" :trash="trash"
        :view="view"
        :restore-loading="restoreLoading" :trash-loading="trashLoading" />
      <image-video-list-skeleton v-if="loading && items.length === 0" :limit="limit" :is-phone="isPhone" />
    </div>

    <NoDataPlaceholder v-if="!loading && items.length === 0" :loading="loading" :permissions="app.permissions" permission="WRITE_EXTERNAL_STORAGE" />
    <v-pagination v-if="!scrollMode && total > limit" :page="page" :go="gotoPage" :total="total" :limit="limit" :page-size="limit" :on-change-page-size="onChangePageSize" />
    <div v-if="scrollMode" ref="sentinel" class="scroll-sentinel"></div>
    <input ref="fileInput" style="display: none" type="file" accept="video/*" multiple @change="uploadChanged" />
    <input ref="dirFileInput" style="display: none" type="file" accept="video/*" multiple webkitdirectory mozdirectory directory @change="dirUploadChanged" />
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import toast from '@/components/toaster'
import { formatSeconds, formatFileSize } from '@/lib/format'
import { videosGQL, initLazyQuery } from '@/lib/api/query'
import { getFileId, getFileUrl, getFileExtension, getFileName } from '@/lib/api/file'
import type { IVideo, IVideoItem } from '@/lib/interfaces'
import type { ISource } from '@/components/lightbox/types'
import { DataType, FEATURE } from '@/lib/data'
import { getSortItems, getVideoGroupByItems, isVideo } from '@/lib/file'
import { hasFeature } from '@/lib/feature'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { useGroupedScroll, type MediaGroup } from '@/hooks/grouped-scroll'
import { useMediaPage } from '@/hooks/media-page'
import { useMediaPageActions } from '@/hooks/media-page-actions'
import MediaPageActions from '@/components/media/MediaPageActions.vue'
import MediaGridItem from '@/components/media/MediaGridItem.vue'
import MediaToolbar from '@/components/media/MediaToolbar.vue'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import { useOpenMedia } from '@/hooks/open-media'

const { videoSortBy, videosCardView, videosGroupBy, videosScrollPaging } = storeToRefs(useMainStore())
const items = ref<IVideoItem[]>([])
const sortItems = getSortItems()
const groupByItems = getVideoGroupByItems()
const imageErrorIds = ref<string[]>([])

const mp = useMediaPage({
  dataType: DataType.VIDEO, routePath: 'videos',
  items, sortByRef: videoSortBy, fileFilter: isVideo,
  downloadName: 'videos', uploadModalId: 'upload-directory-picker-videos', uploadStorageKey: 'videos',
  doFetch: () => fetch(), getScrollMode: () => scrollMode.value,
  setupScroll: () => setTimeout(setupSentinelObserver, 100), teardownScroll: () => teardownObserver(),
  onSort: () => { noMore.value = false },
})
const {
  isPhone, mainStore, tempStore, app, urlTokenKey,
  filter, page, q, limit, dataType,
  fileInput, dirFileInput, uploadChanged, dirUploadChanged, dropping, fileDragEnter, fileDragLeave,
  tags, bucketsMap, deleteItems, deleteItem, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems, viewBucket,
  selectedIds, allChecked, realAllChecked, selectRealAll, allCheckedAlertVisible,
  clearSelection, toggleAllChecked, toggleSelect, total, checked, shiftEffectingIds, handleItemClick, shouldSelect, groupSelectionState, setGroupChecked, toggleGroupChecked,
  downloadItems, downloadFile, trashLoading, trash, restoreLoading, restore,
  gotoPage, onChangePageSize, getQuery, sort, handleMouseOverMode,
  uploadFilesClick, uploadDirClick, dropFiles2,
  uploadDir, uploadDirEditable, editUploadDir,
} = mp

const { noMore, sentinel, isGroupMode, scrollMode, groupedItems, setupSentinelObserver, teardownObserver, prefetchNext } = useGroupedScroll({
  items, getLoading: () => loading.value, limit, page,
  doFetch: () => fetch(), getScrollPaging: () => mainStore.videosScrollPaging, getGroupBy: () => mainStore.videosGroupBy,
})

const effectiveIsGroupMode = computed(() => isGroupMode.value && !filter.text)

const groupItems = (group: MediaGroup<IVideoItem>) => group.entries.map((e) => e.item)
const groupCheckboxState = (group: MediaGroup<IVideoItem>) => groupSelectionState(groupItems(group))
const toggleGroup = (event: Event, group: MediaGroup<IVideoItem>) => toggleGroupChecked(event, groupItems(group))
const toggleGroupByText = (group: MediaGroup<IVideoItem>) => setGroupChecked(groupItems(group), !groupCheckboxState(group).checked)

watch(() => mainStore.videosGroupBy, () => { page.value = 1; noMore.value = false; items.value = []; fetch() })
watch(() => mainStore.videosScrollPaging, () => { page.value = 1; items.value = []; fetch() })

const onImageError = (id: string) => { imageErrorIds.value.push(id) }

const effectiveQ = computed(() => {
  const dirs = mainStore.excludedDirs
  if (!dirs.length) return q.value
  const dirParts = dirs.map((d) => (d.includes(' ') ? `excluded_dir:"${d}"` : `excluded_dir:${d}`))
  return [q.value, ...dirParts].filter(Boolean).join(' ')
})

const { loading, fetch } = initLazyQuery({
  handle: async (data: { videos: IVideo[]; videoCount: number }, error: string) => {
    mp.sorting.value = false
    if (error) { toast(mp.q.value, 'error') } else if (data) {
      const raw = data.videos.map((it) => ({ ...it, fileId: getFileId(urlTokenKey.value, it.path, it.id) }))
      if (scrollMode.value && page.value > 1) {
        const existingIds = new Set(items.value.map((i) => i.id))
        items.value = items.value.concat(raw.filter((i) => !existingIds.has(i.id)))
      } else { items.value = raw }
      total.value = data.videoCount
      if (scrollMode.value) { noMore.value = raw.length < limit.value; prefetchNext() }
    }
  },
  document: videosGQL,
  variables: () => ({ offset: (page.value - 1) * limit.value, limit: limit.value, query: effectiveQ.value, sortBy: effectiveIsGroupMode.value ? 'TAKEN_AT_DESC' : videoSortBy.value }),
})

const sources = computed<ISource[]>(() => items.value.map((it: IVideoItem) => ({
  src: getFileUrl(it.fileId), name: getFileName(it.path), duration: it.duration, size: it.size, path: it.path, data: it, type: dataType,
})) as ISource[])
const { open: openMedia } = useOpenMedia(sources)
function view(index: number) { openMedia(index) }

const actionsProps = useMediaPageActions({
  filterTrash: computed(() => !!filter.trash),
  checked,
  sortBy: videoSortBy,
  sortItems,
  onSort: sort,
  upload: { dir: uploadDir, editable: uploadDirEditable, onUploadFiles: uploadFilesClick, onUploadDir: uploadDirClick, onEditDir: editUploadDir },
  view: { cardView: videosCardView, hide: isGroupMode, onUpdateCardView: (v: boolean) => { videosCardView.value = v } },
  options: {
    show: true,
    showGroupBy: true,
    groupByItems,
    groupBy: videosGroupBy,
    scrollPaging: videosScrollPaging,
    onUpdateGroupBy: (v: string) => { videosGroupBy.value = v },
    onUpdateScrollPaging: (v: boolean) => { videosScrollPaging.value = v },
  },
})
</script>
