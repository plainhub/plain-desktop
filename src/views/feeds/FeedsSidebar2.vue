<template>
  <Sidebar2 :detail="!!route.params.id">
    <ListTopBar
      :title="$t('page_title.feeds')"
      :total="total"
      :selected-ids="selectedIds"
      :checked="checked"
      :all-checked="allChecked"
      :real-all-checked="realAllChecked"
      :toggle-all-checked="toggleAllChecked"
      :alert="{ limit, visible: allCheckedAlertVisible, selectRealAll, clearSelection }"
    >
      <template #bulk>
        <bulk-delete-button :confirming="confirmingDelete" :count="deleteCount" :loading="deleteLoading" @click="deleteItems(selectedIds, realAllChecked, total, q)" @confirm="doDeleteItems" @cancel="cancelDeleteItems" />
        <BulkTagDropdown :type="dataType" :tags="tags" :items="items" :selected-ids="selectedIds" :real-all-checked="realAllChecked" :q="q" />
        <v-icon-button v-tooltip="$t('save_to_notes')" :loading="savingNotes" @click.prevent="saveFeedsToNotes">
          <i-material-symbols:add-notes-outline-rounded />
        </v-icon-button>
      </template>
      <template #actions>
        <v-icon-button v-tooltip="$t('sync_feeds')" :loading="feedsSyncing" @click.prevent="syncFeeds">
          <i-material-symbols:sync-rounded />
        </v-icon-button>
      </template>
    </ListTopBar>
    <div v-if="listLoading && items.length === 0" class="scroller">
      <Sidebar2ItemSkeleton v-for="i in 20" :key="i" :index="i" :is-phone="isPhone" />
    </div>
    <VirtualList v-if="items.length > 0" class="scroller" :data-key="'id'" :data-sources="items" :estimate-size="100" @tobottom="loadMore">
      <template #item="{ index, item }">
        <Sidebar2ListItem
          :item="item"
          :index="index"
          :href="viewUrl(item)"
          :selected="selectedIds.includes(item.id) || item.id == $route.params['id']"
          :selecting="shiftEffectingIds.includes(item.id)"
          :checkbox-checked="shiftEffectingIds.includes(item.id) ? shouldSelect : selectedIds.includes(item.id)"
          @item-click="handleItemClick($event, item, index, () => { view(item) })"
          @mouse-over="handleMouseOver($event, index)"
          @toggle-select="toggleSelect($event, item, index)"
        >
          <template #title>{{ item.title || $t('no_content') }}</template>
          <template #info>
            <a @click.stop.prevent="viewFeed(feedsMap[item.feedId])">{{ feedsMap[item.feedId]?.name }}</a>
            <span>·</span>
            <span v-tooltip="formatDateTime(item.publishedAt)" class="time">
              {{ formatTimeAgo(item.publishedAt) }}
            </span>
            <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
          </template>
          <template #end>
            <img v-if="item.image" class="image" :src="getFileUrl(item.image, '&w=512&h=512')" />
          </template>
        </Sidebar2ListItem>
      </template>
      <template #footer>
        <v-circular-progress v-if="!noMore" indeterminate class="sm" />
      </template>
    </VirtualList>
    <NoDataPlaceholder v-if="!listLoading && items.length === 0" :loading="listLoading" />
  </Sidebar2>
</template>

<script setup lang="ts">
import { inject } from 'vue'
import { formatTimeAgo, formatDateTime } from '@/lib/format'
import { getFileUrl } from '@/lib/api/file'
import VirtualList from '@/components/virtualscroll'
import { useFeedEntriesData } from './hooks/useFeedEntriesData'
import { useFeedEntriesActions } from './hooks/useFeedEntriesActions'

const isPhone = inject('isPhone')

const {
  items, filter, q, tags, feedsMap, feedsSyncing,
  listLoading, loadMore, noMore, fetch, dataType, route, limit,
  selectedIds, allChecked, realAllChecked, selectRealAll, allCheckedAlertVisible,
  clearSelection, toggleAllChecked, toggleSelect, total, checked,
  shiftEffectingIds, handleItemClick, handleMouseOver, shouldSelect,
} = useFeedEntriesData(() => {
  deleteItems(selectedIds.value, realAllChecked.value, total.value, q.value)
})

const {
  deleteItems, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems, saveFeedsToNotes, savingNotes, syncFeeds,
  viewUrl, view, viewFeed,
} = useFeedEntriesActions({ selectedIds, realAllChecked, q, total, tags, items, clearSelection, fetch })
</script>
