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
            <img v-if="item.image" class="image" :src="imageUrl(item.image)" />
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
import { inject, onBeforeUnmount, ref, watch } from 'vue'
import { formatTimeAgo, formatDateTime } from '@/lib/format'
import { resolveFileUrlByPath } from '@/lib/api/file'
import type { IFeedEntry } from '@/lib/interfaces'
import { closeNativeResources, isExternalReference } from '@/hooks/native-resource-url'
import { useTempStore } from '@/stores/temp'
import VirtualList from '@/components/virtualscroll'
import { useFeedEntriesData } from './hooks/useFeedEntriesData'
import { useFeedEntriesActions } from './hooks/useFeedEntriesActions'

const isPhone = inject('isPhone')
const tempStore = useTempStore()

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

const imageUrls = ref<Record<string, string>>({})
const openReferences = ref<string[]>([])
let refreshToken = 0

function imageUrl(reference: string) {
  return imageUrls.value[reference] ?? ''
}

async function refreshImages(entries: IFeedEntry[]) {
  const token = ++refreshToken
  const wanted = [...new Set(entries.map((entry) => entry.image).filter((reference) => !!reference))]
  const stale = openReferences.value.filter((reference) => !wanted.includes(reference))
  const resolved: Record<string, string> = {}
  const opened: string[] = []
  await Promise.all(wanted.map(async (reference) => {
    if (isExternalReference(reference)) {
      resolved[reference] = reference
      return
    }
    const url = await resolveFileUrlByPath(tempStore.urlTokenKey, reference, undefined, '&w=512&h=512')
    resolved[reference] = url
    if (url.startsWith('plain-resource://')) {
      opened.push(reference)
    }
  }))
  if (token !== refreshToken) {
    return
  }
  imageUrls.value = resolved
  openReferences.value = opened
  if (stale.length) {
    await closeNativeResources(stale)
  }
}

watch(items, (entries) => void refreshImages(entries), { immediate: true })

onBeforeUnmount(() => {
  const stale = openReferences.value
  openReferences.value = []
  if (stale.length) {
    void closeNativeResources(stale)
  }
})
</script>
