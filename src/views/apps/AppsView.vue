<template>
  <div class="top-app-bar">
    <v-checkbox touch-target="wrapper" :checked="allChecked" :indeterminate="!allChecked && checked" @change="toggleAllChecked" />
    <div class="title">
      <span v-if="selectedIds.length">{{ $t('x_selected', { count: realAllChecked ? total.toLocaleString() : selectedIds.length.toLocaleString() }) }}</span>
      <span v-else>{{ $t('page_title.apps') }} ({{ total.toLocaleString() }})</span>
      <template v-if="checked">
        <bulk-download-button :single="realAllChecked || selectedIds.length === 1" @download="downloadItems(realAllChecked, selectedIds, q)" @download-each="downloadItemsEach(selectedIds)" @download-zip="downloadItems(realAllChecked, selectedIds, q)" />
      </template>
    </div>

    <div v-if="!isPhone || !checked" class="actions">
      <AppsActionButtons
        :sorting="sorting"
        :sort-items="sortItems"
        :app-sort-by="appSortBy"
        @install="install"
        @sort="sort"
      />
    </div>
  </div>

  <all-checked-alert
    :limit="limit"
    :total="total"
    :all-checked-alert-visible="allCheckedAlertVisible"
    :real-all-checked="realAllChecked"
    :select-real-all="selectRealAll"
    :clear-selection="clearSelection"
  />
  <div class="scroll-content" @dragover.stop.prevent="fileDragEnter">
    <div v-show="dropping" class="drag-mask" @drop.stop.prevent="dropApkFiles" @dragleave.stop.prevent="fileDragLeave">{{ $t('release_to_send_files') }}</div>
    <div class="main-list" :class="{ 'select-mode': checked }">
      <AppListItem
        v-for="(item, i) in items"
        :key="item.id"
        :item="item"
        :index="i"
        :selected-ids="selectedIds"
        :shift-effecting-ids="shiftEffectingIds"
        :should-select="shouldSelect"
        :is-phone="isPhone"
        :handle-item-click="handleItemClick"
        :handle-mouse-over="handleMouseOver"
        :toggle-select="toggleSelect"
        @uninstall="uninstall"
        @download="downloadApp"
        @cancel-uninstall="cancelUninstall"
      />
      <template v-if="loading && items.length === 0">
        <AppSkeletonItem v-for="i in 20" :key="i" :index="i" :is-phone="isPhone" />
      </template>
    </div>
    <NoDataPlaceholder v-if="!loading && items.length === 0" :loading="loading" :permissions="app.permissions" permission="QUERY_ALL_PACKAGES" />
    <v-pagination v-if="total > limit" :page="page" :go="gotoPage" :total="total" :limit="limit" :page-size="limit" :on-change-page-size="onChangePageSize" />
    <input ref="fileInput" style="display: none" type="file" accept=".apk,.apkm,.apks,.xapk" multiple @change="uploadChanged" />
  </div>
</template>

<script setup lang="ts">
import { inject } from 'vue'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import { getSortItems } from '@/lib/file'
import AppsActionButtons from '@/views/apps/AppsActionButtons.vue'
import AppSkeletonItem from '@/views/apps/AppSkeletonItem.vue'
import AppListItem from '@/views/apps/AppListItem.vue'
import { useAppsData } from './hooks/useAppsData'
import { useAppsActions } from './hooks/useAppsActions'

const isPhone = inject('isPhone') as boolean
const sortItems = getSortItems()

const {
  items, page, limit, q, loading, fetch, sorting, isActive, app, appSortBy,
  selectedIds, allChecked, realAllChecked, selectRealAll, allCheckedAlertVisible,
  clearSelection, toggleAllChecked, toggleSelect, total, checked,
  shiftEffectingIds, handleItemClick, handleMouseOver, shouldSelect,
  gotoPage, onChangePageSize, sort, applyRouteQuery,
  pageKeyDown, pageKeyUp,
} = useAppsData()

const {
  fileInput, uploadChanged, dropping, fileDragEnter, fileDragLeave,
  downloadItems, downloadItemsEach, install, uninstall, cancelUninstall, downloadApp, dropApkFiles,
} = useAppsActions({ items, isActive, fetch, applyRouteQuery, clearSelection, pageKeyDown, pageKeyUp })
</script>
<style scoped lang="scss">
:deep(.app-item) {
  grid-template-areas:
    'start image title actions time'
    'start image subtitle  actions time';
  grid-template-columns: 48px 50px 2fr 1fr minmax(240px, auto);
  .image {
    width: 50px;
    height: 50px;
    grid-area: image;
    object-fit: cover;
    border-radius: 8px;
    margin-block: 12px;
  }
  .title {
    margin-inline: 16px;
    padding-block-start: 12px;
  }
  .subtitle {
    grid-area: subtitle;
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 0.875rem;
    margin-inline: 16px;
  }
  .time {
    grid-area: time;
    display: flex;
    flex-direction: column;
    padding-inline: 16px;
    justify-content: center;
    align-items: end;
    gap: 8px;
    font-size: 0.875rem;
  }
}
</style>
