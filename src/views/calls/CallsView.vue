<template>
  <div class="top-app-bar">
    <v-checkbox touch-target="wrapper" :checked="allChecked" :indeterminate="!allChecked && checked" @change="toggleAllChecked" />
    <div class="title">
      <span v-if="selectedIds.length">{{ $t('x_selected', { count: realAllChecked ? total.toLocaleString() : selectedIds.length.toLocaleString() }) }}</span>
      <span v-else>{{ $t('page_title.calls') }} ({{ total.toLocaleString() }})</span>
      <template v-if="checked">
        <bulk-delete-button :confirming="confirmingDelete" :count="deleteCount" :loading="deleteLoading" @click="deleteItems(selectedIds, realAllChecked, total, q)" @confirm="doDeleteItems" @cancel="cancelDeleteItems" />
        <BulkTagDropdown :type="dataType" :tags="tags" :items="items" :selected-ids="selectedIds" :real-all-checked="realAllChecked" :q="q" />
      </template>
    </div>

    <div class="actions">
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
  <div class="scroll-content">
    <div class="main-list" :class="{ 'select-mode': checked }">
      <CallListItem
        v-for="(item, i) in items"
        :key="item.id"
        :item="item"
        :index="i"
        :selected-ids="selectedIds"
        :shift-effecting-ids="shiftEffectingIds"
        :should-select="shouldSelect"
        :is-phone="isPhone"
        :data-type="dataType"
        :tags="tags"
        :call-loading="callLoading"
        :call-id="callId"
        :handle-item-click="handleItemClick"
        :handle-mouse-over="handleMouseOver"
        :toggle-select="toggleSelect"
        @delete-item="deleteItem"
        @call="call"
      />
      <template v-if="loading && items.length === 0">
        <CallSkeletonItem v-for="i in 20" :key="i" :index="i" :is-phone="isPhone" />
      </template>
    </div>
    <NoDataPlaceholder v-if="!loading && items.length === 0" :loading="loading" :access="app.access" feature="CALLS" />
    <v-pagination v-if="total > limit" :page="page" :go="gotoPage" :total="total" :limit="limit" :page-size="limit" :on-change-page-size="onChangePageSize" />
  </div>
</template>

<script setup lang="ts">
import { inject } from 'vue'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import CallListItem from '@/views/calls/CallListItem.vue'
import { useCallsData } from './hooks/useCallsData'
import { useCallsActions } from './hooks/useCallsActions'

const isPhone = inject('isPhone') as boolean

const {
  items, page, limit, q, loading, tags, dataType, app,
  deleteItems, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems,
  selectedIds, allChecked, realAllChecked, selectRealAll, allCheckedAlertVisible,
  clearSelection, toggleAllChecked, toggleSelect, total, checked,
  shiftEffectingIds, handleItemClick, handleMouseOver, shouldSelect,
  gotoPage, onChangePageSize,
} = useCallsData()

const { callId, callLoading, call, deleteItem } = useCallsActions({ items, tags, total })
</script>
<style scoped lang="scss">
:deep(.call-item) {
  grid-template-areas:
    'start title actions geo time'
    'start subtitle actions geo time';
  grid-template-columns: 48px 2fr 1fr minmax(64px, 1fr) minmax(64px, 1fr);
  .title {
    margin-inline-end: 16px;
    padding-block: 8px;
  }
  .subtitle {
    grid-area: subtitle;
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 0.875rem;
    margin-inline-end: 16px;
  }

  .geo {
    grid-area: geo;
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
