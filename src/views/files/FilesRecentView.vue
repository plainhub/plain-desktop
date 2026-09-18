<template>
  <div class="top-app-bar">
    <v-checkbox touch-target="wrapper" :checked="allChecked" :indeterminate="!allChecked && checked" @change="toggleAllChecked" />
    <span v-if="selectedIds.length">{{ $t('x_selected', { count: realAllChecked ? total.toLocaleString() : selectedIds.length.toLocaleString() }) }}</span>
    <span v-else>{{ $t('recent_files') }} ({{ total.toLocaleString() }})</span>
    <template v-if="checked">
      <bulk-download-button :single="selectedIds.length === 1" @download="downloadSelectedItem" @download-each="downloadEachItems" @download-zip="downloadZipItems" />
    </template>
  </div>
  <div v-if="loading && items.length === 0" class="scroller main-list">
    <FileRecentSkeletonItem v-for="i in 20" :key="i" :index="i" :is-phone="isPhone" />
  </div>
  <VirtualList v-if="items.length > 0" class="scroller main-list" :data-key="'id'" :data-sources="items" :estimate-size="isPhone ? 120 : 80">
    <template #item="{ index, item }">
      <FileRecentItem
        :key="item.id"
        :item="item"
        :index="index"
        :selected-ids="selectedIds"
        :shift-effecting-ids="shiftEffectingIds"
        :should-select="shouldSelect"
        :is-phone="isPhone"
        :image-error-ids="imageErrorIds"
        :extension-image-error-ids="extensionImageErrorIds"
        :handle-item-click="handleItemClick"
        :handle-mouse-over="handleMouseOver"
        :toggle-select="toggleSelect"
        :on-image-error="onImageError"
        :on-extension-image-error="onExtensionImageError"
        :download-file="downloadFile"
        :click-item="clickItem"
      />
    </template>
  </VirtualList>
  <NoDataPlaceholder v-if="!loading && items.length === 0" :loading="loading" :permissions="app.permissions" permission="WRITE_EXTERNAL_STORAGE" />
</template>

<script setup lang="ts">
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import VirtualList from '@/components/virtualscroll'
import { useFilesRecent } from '@/hooks/files-recent'

const {
  app, items, loading, isPhone,
  selectedIds, allChecked, realAllChecked, checked, total,
  shiftEffectingIds, shouldSelect, imageErrorIds, extensionImageErrorIds,
  toggleAllChecked, toggleSelect, handleItemClick, handleMouseOver,
  onImageError, onExtensionImageError, downloadFile, clickItem, downloadSelectedItem, downloadEachItems, downloadZipItems,
} = useFilesRecent()
</script>
