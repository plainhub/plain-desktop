<template>
  <MediaToolbar
    page-title="page_title.audios"
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
      <v-icon-button v-tooltip="$t('add_to_playlist')" @click.stop="addItemsToPlaylist($event, selectedIds, realAllChecked, q)">
        <i-material-symbols:playlist-add />
      </v-icon-button>
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
      <AudioListItem
v-for="(item, i) in items" :key="item.id" :item="item" :index="i" :is-phone="isPhone"
        :selected-ids="selectedIds" :shift-effecting-ids="shiftEffectingIds" :should-select="shouldSelect"
        :image-error-ids="imageErrorIds" :buckets-map="bucketsMap" :filter="filter" :data-type="dataType" :tags="tags"
        :animating-ids="animatingIds" :play-loading="playLoading" :play-path="playPath" :main-store="mainStore" :app="app"
        :handle-item-click="handleItemClick" :handle-mouse-over="handleMouseOverMode" :toggle-select="toggleSelect"
        :on-image-error="onImageError" :view-bucket="viewBucket" :delete-item="deleteItem" :restore="restore"
        :download-file="downloadFile" :trash="trash" :handle-remove-from-playlist="handleRemoveFromPlaylist"
        :add-to-playlist="handleAddToPlaylist" :play="play" :pause="pause"
        :is-audio-playing="isAudioPlaying" :is-in-playlist="isInPlaylist"
        :restore-loading="restoreLoading" :trash-loading="trashLoading" />
      <template v-if="loading && items.length === 0">
        <AudioSkeletonItem v-for="i in 20" :key="i" :index="i" :is-phone="isPhone" />
      </template>
    </div>
    <NoDataPlaceholder v-if="!loading && items.length === 0" :loading="loading" :permissions="app.permissions" permission="WRITE_EXTERNAL_STORAGE" />
    <v-pagination v-if="!scrollMode && total > limit" :page="page" :go="gotoPage" :total="total" :limit="limit" :page-size="limit" :on-change-page-size="onChangePageSize" />
    <div v-if="scrollMode" ref="sentinel" class="scroll-sentinel"></div>
    <input ref="fileInput" style="display: none" type="file" accept="audio/*" multiple @change="uploadChanged" />
    <input ref="dirFileInput" style="display: none" type="file" accept="audio/*" multiple webkitdirectory mozdirectory directory @change="dirUploadChanged" />
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import toast from '@/components/toaster'
import { audiosGQL, initLazyQuery } from '@/lib/api/query'
import type { IAudio, IAudioItem } from '@/lib/interfaces'
import { DataType, FEATURE } from '@/lib/data'
import { getSortItems, isAudio } from '@/lib/file'
import { hasFeature } from '@/lib/feature'
import { getFileId } from '@/lib/api/file'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { useAddToPlaylist, useAudioPlayer } from './hooks/useAudiosHooks'
import { useMediaPage } from '@/hooks/media-page'
import { useMediaPageActions } from '@/hooks/media-page-actions'
import { useMoveToFolder } from '@/hooks/media'
import MediaPageActions from '@/components/media/MediaPageActions.vue'
import MediaToolbar from '@/components/media/MediaToolbar.vue'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import AudioListItem from './AudioListItem.vue'
import AudioSkeletonItem from './AudioSkeletonItem.vue'

const mainStoreLocal = useMainStore()
const tempStoreLocal = useTempStore()
const { audioSortBy, audiosScrollPaging } = storeToRefs(mainStoreLocal)
const { audioPlaying } = storeToRefs(tempStoreLocal)
const items = ref<IAudioItem[]>([])
const sortItems = getSortItems()
const imageErrorIds = ref<string[]>([])
const animatingIds = ref<string[]>([])

const scrollMode = computed(() => mainStoreLocal.audiosScrollPaging)
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
  dataType: DataType.AUDIO, routePath: 'audios',
  items, sortByRef: audioSortBy, fileFilter: isAudio,
  downloadName: 'audios', uploadModalId: 'upload-directory-picker-audios', uploadStorageKey: 'audios',
  doFetch: () => fetch(), getScrollMode: () => scrollMode.value,
  setupScroll: () => setTimeout(setupSentinelObserver, 100),
  teardownScroll: () => { observer?.disconnect(); observer = null },
  onSort: () => { 
    noMore.value = false 
  },
})
const {
  isPhone, mainStore, app, urlTokenKey,
  filter, page, q, limit, dataType,
  fileInput, dirFileInput, uploadChanged, dirUploadChanged, dropping, fileDragEnter, fileDragLeave,
  bucketsMap, deleteItems, deleteItem, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems, viewBucket, tags,
  selectedIds, allChecked, realAllChecked, selectRealAll, allCheckedAlertVisible,
  clearSelection, toggleAllChecked, toggleSelect, total, checked, shiftEffectingIds, handleItemClick, shouldSelect,
  downloadItems, downloadFile, trashLoading, trash, restoreLoading, restore,
  gotoPage, onChangePageSize, getQuery, sort, handleMouseOverMode,
  uploadFilesClick, uploadDirClick, dropFiles2,
  uploadDir, uploadDirEditable, editUploadDir,
} = mp

const isAudioPlaying = (item: IAudioItem) => audioPlaying.value && app.value?.audioCurrent === item.path
const effectiveQ = computed(() => {
  const dirs = mainStoreLocal.excludedDirs
  if (!dirs.length) return q.value
  const dirParts = dirs.map((d) => (d.includes(' ') ? `excluded_dir:"${d}"` : `excluded_dir:${d}`))
  return [q.value, ...dirParts].filter(Boolean).join(' ')
})
const { play, playPath, loading: playLoading, pause } = useAudioPlayer()
const { moveLoading, onMoveClick } = useMoveToFolder('move-audios-picker')
const { addItemsToPlaylist, addToPlaylist, removeFromPlaylist, isInPlaylist } = useAddToPlaylist(items, clearSelection)
const onImageError = (id: string) => { imageErrorIds.value.push(id) }

function handleRemoveFromPlaylist(e: MouseEvent, item: IAudioItem) {
  animatingIds.value.push(item.id)
  setTimeout(() => { removeFromPlaylist(e, item); setTimeout(() => { animatingIds.value = animatingIds.value.filter((id) => id !== item.id) }, 200) }, 150)
}
function handleAddToPlaylist(e: MouseEvent, item: IAudioItem) {
  animatingIds.value.push(item.id)
  setTimeout(() => { addToPlaylist(e, item); setTimeout(() => { animatingIds.value = animatingIds.value.filter((id) => id !== item.id) }, 200) }, 150)
}

const { loading, fetch } = initLazyQuery({
  handle: (data: { items: IAudio[]; total: number }, error: string) => {
    mp.sorting.value = false
    if (error) { toast(mp.q.value, 'error') } else if (data) {
      const raw = data.items.map((it) => ({ ...it, fileId: getFileId(urlTokenKey.value, it.path, it.id) }))
      if (scrollMode.value && page.value > 1) { items.value = items.value.concat(raw) }
      else { items.value = raw }
      total.value = data.total
      if (scrollMode.value) { noMore.value = data.items.length < limit.value }
    }
  },
  document: audiosGQL,
  variables: () => ({ offset: (page.value - 1) * limit.value, limit: limit.value, query: effectiveQ.value, sortBy: audioSortBy.value }),
})

const actionsProps = useMediaPageActions({
  filterTrash: computed(() => !!filter.trash),
  checked,
  sortBy: audioSortBy,
  sortItems,
  onSort: sort,
  upload: { dir: uploadDir, editable: uploadDirEditable, onUploadFiles: uploadFilesClick, onUploadDir: uploadDirClick, onEditDir: editUploadDir },
  options: {
    show: true,
    scrollPaging: audiosScrollPaging,
    onUpdateScrollPaging: (v: boolean) => { audiosScrollPaging.value = v },
  },
})
</script>

<style scoped lang="scss">
:deep(.media-item) {
  grid-template-areas:
    'start image title actions artist time'
    'start image subtitle  actions artist time';
  grid-template-columns: 48px 50px 2fr 240px minmax(64px, 1fr) minmax(140px, auto);
  &:hover {
    cursor: pointer;
  }
  .image {
    width: 50px;
    height: 50px;
    grid-area: image;
    margin-block: 12px;
    text-align: center;
    .svg {
      max-width: 50px;
      max-height: 50px;
    }
  }
  .title {
    margin-inline: 16px;
    padding-block-start: 12px;
  }
  .subtitle {
    grid-area: subtitle;
    margin-inline: 16px;
    margin-block-start: 8px;
    margin-block-end: 12px;
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 0.875rem;
  }
  .artist {
    grid-area: artist;
    display: flex;
    align-items: center;
  }
  .time {
    grid-area: time;
    display: flex;
    align-items: center;
    padding-inline: 16px;
    justify-content: end;
  }
}
</style>