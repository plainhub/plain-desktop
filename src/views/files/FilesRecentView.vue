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
      <FilePreviewListItem :key="item.id" :item="item" :index="index" :is-phone="isPhone" :list="list" />
    </template>
  </VirtualList>
  <NoDataPlaceholder v-if="!loading && items.length === 0" :placeholder-key="listError" :loading="loading" :access="app.access" feature="FILES" />
</template>

<script setup lang="ts">
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import VirtualList from '@/components/virtualscroll'
import FilePreviewListItem from './FilePreviewListItem.vue'
import { useFilesRecent } from '@/hooks/files-recent'

const {
  app, items, list, listError, loading, isPhone, downloadSelectedItem, downloadEachItems, downloadZipItems,
} = useFilesRecent()
const { selectedIds, allChecked, realAllChecked, checked, total, toggleAllChecked } = list.selection
</script>
