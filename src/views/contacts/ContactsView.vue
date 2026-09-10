<template>
  <div class="top-app-bar">
    <v-checkbox touch-target="wrapper" :checked="allChecked" :indeterminate="!allChecked && checked" @change="toggleAllChecked" />
    <div class="title">
      <span v-if="selectedIds.length">{{ $t('x_selected', { count: realAllChecked ? total.toLocaleString() : selectedIds.length.toLocaleString() }) }}</span>
      <span v-else>{{ $t('page_title.contacts') }} ({{ total.toLocaleString() }})</span>
      <template v-if="checked">
        <bulk-delete-button :confirming="confirmingDelete" :count="deleteCount" :loading="deleteLoading" @click="deleteItems(selectedIds, realAllChecked, total, q)" @confirm="doDeleteItems" @cancel="cancelDeleteItems" />
        <v-icon-button v-tooltip="$t('download')" style="display: none">
          <i-material-symbols:download-rounded />
        </v-icon-button>
        <BulkTagDropdown :type="dataType" :tags="tags" :items="items" :selected-ids="selectedIds" :real-all-checked="realAllChecked" :q="q" />
      </template>
    </div>
    <div class="actions">
      <v-outlined-button @click="create">
        {{ $t('create') }}
      </v-outlined-button>
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
      <ContactListItem
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
        :call-index="callIndex"
        :handle-item-click="handleItemClick"
        :handle-mouse-over="handleMouseOver"
        :toggle-select="toggleSelect"
        @delete-item="deleteItem"
        @edit="edit"
        @send-sms="sendSms"
        @call="call"
      />
      <template v-if="loading && items.length === 0">
        <ContactSkeletonItem v-for="i in 20" :key="i" :index="i" :is-phone="isPhone" />
      </template>
    </div>
    <NoDataPlaceholder v-if="!loading && items.length === 0" :loading="loading" :permissions="app.permissions" permission="WRITE_CONTACTS" />
    <v-pagination v-if="total > limit" :page="page" :go="gotoPage" :total="total" :limit="limit" :page-size="limit" :on-change-page-size="onChangePageSize" />
  </div>
</template>

<script setup lang="ts">
import { inject } from 'vue'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import ContactListItem from '@/views/contacts/ContactListItem.vue'
import { useContactsData } from './hooks/useContactsData'
import { useContactsActions } from './hooks/useContactsActions'

const isPhone = inject('isPhone') as boolean

const {
  items, page, limit, q, loading, tags, dataType, app, sources, fetch,
  deleteItems, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems,
  selectedIds, allChecked, realAllChecked, selectRealAll, allCheckedAlertVisible,
  clearSelection, toggleAllChecked, toggleSelect, total, checked,
  shiftEffectingIds, handleItemClick, handleMouseOver, shouldSelect,
  gotoPage, onChangePageSize,
} = useContactsData()

const {
  callId, callIndex, callLoading, call, sendSms,
  deleteItem, edit, create,
} = useContactsActions({ items, tags, total, sources, fetch })
</script>
<style lang="scss" scoped>
.list-unstyled {
  list-style: none;
  margin: 0;
  padding: 0;
}
:deep(.contact-item) {
  grid-template-areas:
    'start image title info actions time'
    'start image subtitle info actions time';
  grid-template-columns: 48px 50px minmax(100px, 1fr) 1fr 1fr minmax(64px, 1fr);
  .image {
    width: 50px;
    height: 50px;
    grid-area: image;
    object-fit: cover;
    border-radius: 8px;
    margin-block: 8px;
  }
  .title {
    margin-inline: 16px;
    padding-block-start: 8px;
  }
  .subtitle {
    grid-area: subtitle;
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 0.875rem;
    margin-inline: 16px;
  }
  .info {
    grid-area: info;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-inline: 16px;
    padding-block: 12px;
    justify-content: center;
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
