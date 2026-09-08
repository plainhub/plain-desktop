<template>
  <Sidebar2 :detail="!!route.params.id">
    <ListTopBar
      :title="$t('page_title.notes')"
      :total="total"
      :selected-ids="selectedIds"
      :checked="checked"
      :all-checked="allChecked"
      :real-all-checked="realAllChecked"
      :toggle-all-checked="toggleAllChecked"
      :alert="{ limit, visible: allCheckedAlertVisible, selectRealAll, clearSelection }"
    >
      <template #bulk>
        <template v-if="filter.trash">
          <bulk-delete-button :confirming="confirmingDelete" :count="deleteCount" :loading="deleteLoading" @click="deleteItems(selectedIds, realAllChecked, total, q)" @confirm="doDeleteItems" @cancel="cancelDeleteItems" />
          <v-icon-button v-tooltip="$t('restore')" :loading="restoreLoading(getQuery())" @click.stop="restore(getQuery())">
            <i-material-symbols:restore-from-trash-outline-rounded />
          </v-icon-button>
        </template>
        <template v-else>
          <v-icon-button v-tooltip="$t('move_to_trash')" @click.stop="trash(getQuery())">
            <i-material-symbols:delete-outline-rounded />
          </v-icon-button>
          <BulkTagDropdown :type="dataType" :tags="tags" :items="items" :selected-ids="selectedIds" :real-all-checked="realAllChecked" :q="q" />
          <v-icon-button v-tooltip="$t('export_notes')" @click.stop="exportNotes2">
            <i-material-symbols:export-notes-outline-rounded />
          </v-icon-button>
        </template>
      </template>
      <template #actions>
        <v-outlined-button v-if="!filter.trash" class="btn-sm" @click.prevent="create">{{ $t('create') }}</v-outlined-button>
      </template>
    </ListTopBar>
    <div v-if="loading && items.length === 0" class="scroller">
      <Sidebar2ItemSkeleton v-for="i in 20" :key="i" :index="i" :is-phone="isPhone" :image="false" />
    </div>
    <VirtualList v-if="items.length" class="scroller" :data-key="'id'" :data-sources="items" :estimate-size="100" :class="{ 'select-mode': checked }">
      <template #item="{ index, item }">
        <Sidebar2ListItem
          :item="item"
          :index="index"
          :href="viewUrl(item)"
          :selected="selectedIds.includes(item.id) || item.id == route.params['id']"
          :selecting="shiftEffectingIds.includes(item.id)"
          :checkbox-checked="shiftEffectingIds.includes(item.id) ? shouldSelect : selectedIds.includes(item.id)"
          @item-click="handleItemClick($event, item, index, () => { view(item) })"
          @mouse-over="handleMouseOver($event, index)"
          @toggle-select="toggleSelect($event, item, index)"
        >
          <template #title>{{ item.title || $t('meta_no_title') }}</template>
          <template #info>
            <span v-tooltip="formatDateTime(item.updatedAt)" class="time">
              {{ formatTimeAgo(item.updatedAt) }}
            </span>
            <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
          </template>
        </Sidebar2ListItem>
      </template>
      <template #footer>
        <v-pagination v-if="total > limit" :page="page" :go="gotoPage" :total="total" :limit="limit" :page-size="limit" :on-change-page-size="onChangePageSize" />
      </template>
    </VirtualList>
    <NoDataPlaceholder v-if="!loading && items.length === 0" :loading="loading" />
  </Sidebar2>
</template>

<script setup lang="ts">
import { inject } from 'vue'
import VirtualList from '@/components/virtualscroll'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import { formatTimeAgo, formatDateTime } from '@/lib/format'
import { useNotesData } from './hooks/useNotesData'
import { useNotesActions } from './hooks/useNotesActions'

const isPhone = inject('isPhone') as boolean

const {
  items, filter, page, limit, q, loading, fetch, tags, dataType, route,
  selectedIds, allChecked, realAllChecked, selectRealAll, allCheckedAlertVisible,
  clearSelection, toggleAllChecked, toggleSelect, total, checked,
  shiftEffectingIds, handleItemClick, handleMouseOver, shouldSelect,
  gotoPage, onChangePageSize,
} = useNotesData(() => {
  deleteItems(selectedIds.value, realAllChecked.value, total.value, q.value)
})

const {
  deleteItems, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems,
  exportNotes2, getQuery, trashLoading, trash, restoreLoading, restore,
  view, viewUrl, create,
} = useNotesActions({ items, selectedIds, realAllChecked, q, total, clearSelection, fetch })
</script>
