<template>
  <Sidebar2 :detail="!!route.params.threadId">
    <ListTopBar
      v-if="!isArchived"
      :title="$t('page_title.conversations')"
      :total="total"
      :selected-ids="selectedIds"
      :checked="checked"
      :all-checked="allChecked"
      :real-all-checked="realAllChecked"
      :toggle-all-checked="toggleAllChecked"
    >
      <template #bulk>
        <v-icon-button v-tooltip="$t('archive_conversation')" @click.stop="archiveConversations(selectedIds)">
          <i-material-symbols:archive-outline-rounded />
        </v-icon-button>
      </template>
      <template #actions>
        <v-dropdown v-model="sortMenuVisible">
          <template #trigger>
            <v-icon-button v-tooltip="$t('sort')">
              <i-material-symbols:sort-rounded />
            </v-icon-button>
          </template>
          <div
            v-for="item in sortItems"
            :key="item.value"
            class="dropdown-item"
            :class="{ selected: item.value === mainStore.conversationSortBy }"
            @click="mainStore.conversationSortBy = item.value; sortMenuVisible = false"
          >
            {{ $t(item.label) }}
          </div>
        </v-dropdown>
        <v-icon-button v-tooltip="$t('export_sms')" @click.stop="openExport">
          <i-material-symbols:download-rounded />
        </v-icon-button>
        <v-icon-button v-tooltip="$t('send_sms')" @click.stop="openSendSms()">
          <i-material-symbols:sms-outline-rounded />
        </v-icon-button>
      </template>
    </ListTopBar>
    <ListTopBar
      v-else
      :title="$t('archived')"
      :total="total"
      :selected-ids="selectedIds"
      :checked="checked"
      :all-checked="allChecked"
      :real-all-checked="realAllChecked"
      :toggle-all-checked="toggleAllChecked"
    >
      <template #bulk>
        <v-icon-button v-tooltip="$t('unarchive')" @click.stop="unarchiveConversations(selectedIds)">
          <i-material-symbols:unarchive-outline-rounded />
        </v-icon-button>
      </template>
    </ListTopBar>

    <conversation-skeleton-list v-if="loading && conversations.length === 0" />

    <VirtualList v-if="conversations.length > 0" class="scroller" :data-key="'id'" :data-sources="sortedConversations" :estimate-size="80" @tobottom="loadMore">
      <template #item="{ index, item }">
        <a class="item-link" :href="`/messages/${isArchived ? 'archived/' : ''}${item.id}`" @click.prevent="openConversation(item)">
          <article
            class="conversation-item selectable-card"
            :class="{ selected: selectedIds.includes(item.id) || item.id == route.params.threadId, selecting: shiftEffectingIds.includes(item.id) }"
            @click.stop.prevent="handleItemClick($event, item, index, () => openConversation(item))"
            @mouseenter.stop="handleMouseOver($event, index)"
          >
            <div class="title">
              <v-checkbox v-if="shiftEffectingIds.includes(item.id)" class="checkbox" touch-target="wrapper" :checked="shouldSelect" @click.stop="toggleSelect($event, item, index)" />
              <v-checkbox v-else class="checkbox" touch-target="wrapper" :checked="selectedIds.includes(item.id)" @click.stop="toggleSelect($event, item, index)" />
              <div class="text">{{ getConversationDisplayName(item) }}<span class="count">({{ item.messageCount.toLocaleString() }})</span></div>
              <span v-tooltip="formatDateTime(item.date)" class="time">{{ formatTimeAgo(item.date) }}</span>
            </div>
            <div class="subtitle">
              <span class="number"><field-id :id="index + 1" :raw="item" /></span>
              <span class="text">{{ item.snippet || '-' }}</span>
            </div>
          </article>
        </a>
      </template>
      <template #footer>
        <v-circular-progress v-if="!noMore" indeterminate class="sm" />
      </template>
    </VirtualList>

    <template v-if="!loading && conversations.length === 0">
      <NoDataPlaceholder :loading="loading" :permissions="app.permissions" permission="READ_SMS" />
    </template>
  </Sidebar2>
</template>

<script setup lang="ts">
import { formatTimeAgo, formatDateTime } from '@/lib/format'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import VirtualList from '@/components/virtualscroll'
import { sortItems, useMessagesSidebar } from '@/hooks/messages-sidebar'
import ConversationSkeletonList from './ConversationSkeletonList.vue'

const {
  mainStore, app, route, isArchived,
  sortMenuVisible, noMore, conversations, sortedConversations, loading,
  getConversationDisplayName,
  loadMore, openConversation, openSendSms, openExport,
  archiveConversations, unarchiveConversations,
  total, selectedIds, allChecked, realAllChecked, checked, shouldSelect, shiftEffectingIds,
  toggleAllChecked, toggleSelect, handleItemClick, handleMouseOver,
} = useMessagesSidebar()
</script>

<style scoped lang="scss">
:deep(.conversation-item) {
  margin: 0 16px 8px 16px;
  display: grid;
  box-sizing: border-box;
  border-radius: 8px;
  grid-template-areas:
    'title'
    'subtitle';
  grid-template-columns: 1fr;

  &:hover {
    cursor: pointer;
  }

  .title {
    grid-area: title;
    display: flex;
    align-items: center;

    .checkbox {
      flex-shrink: 0;
      margin-inline-start: 4px;
    }

    .text {
      font-weight: 500;
      flex: 1;
      width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      margin-block: 8px;
      margin-inline-end: 8px;

      .count {
        font-weight: 400;
        color: var(--md-sys-color-on-surface-variant);
        margin-inline-start: 4px;
        font-size: 0.875rem;
      }
    }

    .time {
      flex-shrink: 0;
      font-size: 0.75rem;
      color: var(--md-sys-color-on-surface-variant);
      margin-inline-end: 12px;
    }
  }

  .subtitle {
    font-size: 0.875rem;
    grid-area: subtitle;
    display: flex;
    align-items: center;
    margin-block-end: 12px;
    margin-inline-end: 16px;
    margin-inline-start: 4px;
    color: var(--md-sys-color-on-surface-variant);

    .number {
      min-width: 40px;
      text-align: center;
      flex-shrink: 0;
    }

    .text {
      flex: 1;
      overflow: hidden;
      width: 0;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }
}
</style>
