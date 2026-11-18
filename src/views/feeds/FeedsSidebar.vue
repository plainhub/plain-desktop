<template>
  <Teleport v-if="isActive" to="#header-start-slot" defer>
    <v-icon-button id="add-feed-ref" v-tooltip="$t('add_subscription')" @click="() => (addMenuVisible = true)">
      <i-material-symbols:add-rounded />
    </v-icon-button>
    <v-dropdown-menu v-model="addMenuVisible" anchor="add-feed-ref">
      <div v-for="item in actionItems" :key="item.text" class="dropdown-item" @click="item.click(); addMenuVisible = false">
        {{ $t(item.text) }}
      </div>
    </v-dropdown-menu>
  </Teleport>
  <left-sidebar>
    <template #body>
      <ul class="nav">
        <SidebarListItem
          :title="$t('all')"
          :active="!today && !selectedTagId && !selectedFeedId"
          @click="viewAll"
        >
          <template #start>
            <i-lucide:layout-grid />
          </template>
          <template v-if="counter.feedEntries >= 0" #end>
            <span class="count">{{ counter.feedEntries.toLocaleString() }}</span>
          </template>
        </SidebarListItem>
        <SidebarListItem
          :title="$t('today')"
          :active="today"
          @click="viewToday"
        >
          <template #start>
            <i-lucide:calendar-days />
          </template>
          <template v-if="counter.feedEntriesToday >= 0" #end>
            <span class="count">{{ counter.feedEntriesToday.toLocaleString() }}</span>
          </template>
        </SidebarListItem>
        <SidebarListItem
          v-for="item in feeds"
          :key="item.id"
          :title="item.name"
          :active="!!selectedFeedId && item.id === selectedFeedId"
          @click="viewFeed(item)"
        >
          <template #end>
            <span v-if="getFeedCount(item.id) >= 0" class="count">{{ getFeedCount(item.id).toLocaleString() }}</span>
          </template>
          <template #actions>
            <v-icon-button :id="'feed-' + item.id" v-tooltip="$t('actions')" class="sm" @click.prevent.stop="showFeedMenu(item)">
              <i-material-symbols:more-horiz />
            </v-icon-button>
          </template>
        </SidebarListItem>
      </ul>
      <v-dropdown-menu v-model="feedMenuVisible" :anchor="'feed-' + selectedFeed?.id">
        <template v-if="!confirmingDeleteFeed">
          <div class="dropdown-item" @click="editFeed(selectedFeed!); feedMenuVisible = false">
            {{ $t('edit') }}
          </div>
          <div class="dropdown-item" @click="deleteFeed(selectedFeed!)">
            {{ $t('delete') }}
          </div>
        </template>
        <template v-else>
          <inline-delete-confirm :name="deletingFeed?.name ?? ''" :loading="deleteFeedLoading" @confirm="doDeleteFeed" @cancel="cancelDeleteFeed" />
        </template>
      </v-dropdown-menu>
      <tag-filter type="FEED_ENTRY" :selected="selectedTagId" />
      <input ref="fileInput" style="display: none" accept=".xml" type="file" @change="uploadChanged" />
    </template>
  </left-sidebar>
</template>

<script setup lang="ts">
import { onActivated, onDeactivated, ref } from 'vue'
import { useFeedsSidebar } from '@/hooks/feeds-sidebar'

const {
  counter, feeds, actionItems,
  addMenuVisible, selectedTagId, selectedFeedId, today,
  fileInput, feedMenuVisible, selectedFeed,
  confirmingDeleteFeed, deletingFeed, deleteFeedLoading,
  getFeedCount, viewFeed, viewAll, viewToday,
  uploadChanged, showFeedMenu, editFeed, deleteFeed, doDeleteFeed, cancelDeleteFeed,
} = useFeedsSidebar()

const isActive = ref(false)

onActivated(() => { isActive.value = true })
onDeactivated(() => { isActive.value = false })
</script>
