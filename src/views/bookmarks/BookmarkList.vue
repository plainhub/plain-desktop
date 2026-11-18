<template>
  <div class="quick-content-main">
    <div class="top-app-bar">
      <v-icon-button v-tooltip="$t('close')" @click="store.quick = ''">
        <i-material-symbols:arrow-back-rounded />
      </v-icon-button>
      <div class="title">{{ $t('bookmarks') }}</div>
      <div class="actions">
        <v-dropdown v-model="sortMenuVisible">
          <template #trigger>
            <v-icon-button v-tooltip="$t('bookmark_sort_order')">
              <i-material-symbols:sort-rounded />
            </v-icon-button>
          </template>
          <div class="dropdown-item" :class="{ selected: bmStore.sortOrder === 'AZ' }" @click="setSortOrder('AZ'); sortMenuVisible = false">
            {{ $t('sort_by.name_asc') }}
          </div>
          <div class="dropdown-item" :class="{ selected: bmStore.sortOrder === 'RECENT_CLICK' }" @click="setSortOrder('RECENT_CLICK'); sortMenuVisible = false">
            {{ $t('bookmark_sort_recent') }}
          </div>
        </v-dropdown>
        <v-dropdown v-model="addMenuVisible">
          <template #trigger>
            <v-icon-button v-tooltip="$t('add_bookmarks')">
              <i-material-symbols:add-rounded />
            </v-icon-button>
          </template>
          <div class="dropdown-item" @click="openAddDialog(''); addMenuVisible = false">
            {{ $t('add_bookmarks') }}
          </div>
          <div class="dropdown-item" @click="openAddGroupDialog(); addMenuVisible = false">
            {{ $t('add_bookmark_group') }}
          </div>
          <div class="dropdown-item" @click="doImport(); addMenuVisible = false">
            {{ $t('import_bookmarks') }}
          </div>
          <div class="dropdown-item" @click="doExport(); addMenuVisible = false">
            {{ $t('export_bookmarks') }}
          </div>
          <div class="dropdown-item danger" @click="clearAllBookmarks(); addMenuVisible = false">
            {{ $t('clear_bookmarks') }}
          </div>
        </v-dropdown>
      </div>
    </div>

    <div class="quick-content-body">
      <!-- Loading -->
      <div v-if="loading" class="loading-state">
        <v-circular-progress indeterminate class="sm" />
      </div>

      <!-- Empty state -->
      <div v-else-if="!bmStore.bookmarks.length && !bmStore.groups.length" class="no-data">
        <div class="empty-content">
          <i-lucide:bookmark class="empty-icon" />
          <div class="empty-text">{{ $t('no_bookmarks') }}</div>
          <v-filled-button @click="openAddDialog('')">
            {{ $t('add_bookmarks') }}
          </v-filled-button>
        </div>
      </div>

      <div v-else class="bookmark-list">
        <!-- Pinned bookmarks (ungrouped or from any group) -->
        <div v-if="pinnedBookmarks.length" class="bookmark-section pinned-section">
          <div class="section-header">
            <i-lucide:pin class="pin-icon" />
            <span>{{ $t('pinned') }}</span>
          </div>
          <BookmarkItem
            v-for="b in pinnedBookmarks"
            :key="b.id"
            :bookmark="b"
            :groups="bmStore.sortedGroups"
            @click="openBookmark(b)"
            @edit="startEdit(b)"
            @delete="deleteBookmark(b.id)"
            @toggle-pin="togglePin(b)"
          />
        </div>

        <!-- Groups -->
        <div v-for="group in bmStore.sortedGroups" :key="group.id" class="bookmark-section">
          <div class="section-header group-header" @click="toggleGroup(group)">
            <i-lucide:chevron-right
              class="collapse-icon"
              :class="{ expanded: !group.collapsed }"
            />
            <span class="group-name">{{ group.name }}</span>
            <v-dropdown v-model="groupMenus[group.id]" @click.stop>
              <template #trigger>
                <v-icon-button class="icon more-trigger">
                  <i-material-symbols:more-horiz />
                </v-icon-button>
              </template>
              <div class="dropdown-item" @click="openAddDialog(group.id); groupMenus[group.id] = false">
                {{ $t('add_bookmarks') }}
              </div>
              <div class="dropdown-item" @click="startEditGroup(group); groupMenus[group.id] = false">
                {{ $t('edit') }}
              </div>
              <div class="dropdown-item danger" @click="clearGroupBookmarks(group.id); groupMenus[group.id] = false">
                {{ $t('clear_group_bookmarks') }}
              </div>
              <div class="dropdown-item danger" @click="deleteGroup(group.id); groupMenus[group.id] = false">
                {{ $t('delete') }}
              </div>
            </v-dropdown>
          </div>
          <transition name="collapse">
            <div v-if="!group.collapsed" class="group-items">
              <BookmarkItem
                v-for="b in nonPinnedBookmarks(group.id)"
                :key="b.id"
                :bookmark="b"
                :groups="bmStore.sortedGroups"
                @click="openBookmark(b)"
                @edit="startEdit(b)"
                @delete="deleteBookmark(b.id)"
                @toggle-pin="togglePin(b)"
              />
              <div v-if="!nonPinnedBookmarks(group.id).length" class="group-empty">
                {{ $t('no_bookmarks_in_group') }}
              </div>
            </div>
          </transition>
        </div>

        <!-- Ungrouped bookmarks (non-pinned) -->
        <div v-if="nonPinnedUngrouped.length" class="bookmark-section">
          <div v-if="bmStore.sortedGroups.length" class="section-header">
            <i-lucide:bookmark />
            <span>{{ $t('ungrouped') }}</span>
          </div>
          <BookmarkItem
            v-for="b in nonPinnedUngrouped"
            :key="b.id"
            :bookmark="b"
            :groups="bmStore.sortedGroups"
            @click="openBookmark(b)"
            @edit="startEdit(b)"
            @delete="deleteBookmark(b.id)"
            @toggle-pin="togglePin(b)"
          />
        </div>
      </div>
    </div>

    <!-- Add bookmarks modal -->
    <AddBookmarksModal
      v-if="addDialogVisible"
      :default-group-id="addDialogGroupId"
      :groups="bmStore.sortedGroups"
      @close="addDialogVisible = false"
      @saved="onBookmarksAdded"
    />

    <!-- Edit bookmark modal -->
    <EditBookmarkModal
      v-if="editBookmark"
      :bookmark="editBookmark"
      :groups="bmStore.sortedGroups"
      @close="editBookmark = null"
      @saved="onBookmarkUpdated"
    />

    <!-- Add/Edit group modal is triggered via openModal in script -->
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useMainStore } from '@/stores/main'
import { useBookmarksStore, type Bookmark } from '@/stores/bookmarks'
import BookmarkItem from './BookmarkItem.vue'
import AddBookmarksModal from './AddBookmarksModal.vue'
import EditBookmarkModal from './EditBookmarkModal.vue'
import { useBookmarkOperations } from './bookmarks'

const store = useMainStore()
const bmStore = useBookmarksStore()

const sortMenuVisible = ref(false)
const addMenuVisible = ref(false)
const groupMenus = ref<Record<string, boolean>>({})
const addDialogVisible = ref(false)
const addDialogGroupId = ref('')
const editBookmark = ref<Bookmark | null>(null)

const {
  loading,
  pinnedBookmarks,
  nonPinnedUngrouped,
  nonPinnedBookmarks,
  openBookmark,
  togglePin,
  deleteBookmark,
  clearAllBookmarks,
  clearGroupBookmarks,
  toggleGroup,
  startEditGroup,
  openAddGroupDialog,
  deleteGroup,
  doExport,
  doImport,
} = useBookmarkOperations()

function setSortOrder(order: 'AZ' | 'RECENT_CLICK') {
  bmStore.sortOrder = order
}

function openAddDialog(groupId: string) {
  addDialogGroupId.value = groupId
  addDialogVisible.value = true
}

function startEdit(b: Bookmark) {
  editBookmark.value = { ...b }
}

function onBookmarksAdded(items: Bookmark[]) {
  addDialogVisible.value = false
  bmStore.addBookmarks(items)
}

function onBookmarkUpdated(updated: Bookmark) {
  editBookmark.value = null
  bmStore.updateBookmark({ ...updated })
}
</script>

<style scoped lang="scss">
.loading-state {
  display: flex;
  justify-content: center;
  padding: 40px;
}

.no-data {
  display: flex;
  justify-content: center;
  padding: 60px 16px;

  .empty-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    color: var(--md-sys-color-on-surface-variant);

    .empty-icon {
      width: 48px;
      height: 48px;
      opacity: 0.4;
    }

    .empty-text {
      font-size: 1rem;
      opacity: 0.7;
    }


  }
}

.bookmark-list {
  padding-bottom: 16px;
}

.bookmark-section {
  margin-top: 4px;
}

.section-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px 4px;
  font-size: 0.8rem;
  font-weight: 600;
  color: var(--md-sys-color-on-surface-variant);
  text-transform: uppercase;
  letter-spacing: 0.5px;

  .pin-icon {
    width: 14px;
    height: 14px;
    color: var(--md-sys-color-primary);
  }
}

.group-header {
  cursor: pointer;
  border-radius: 6px;
  padding: 8px 12px;
  user-select: none;
  text-transform: none;
  letter-spacing: 0;
  font-size: 0.875rem;

  &:hover {
    background: var(--md-sys-color-surface-container-high);
  }

  .collapse-icon {
    width: 16px;
    height: 16px;
    transition: transform 0.2s;

    &.expanded {
      transform: rotate(90deg);
    }
  }

  .group-name {
    flex: 1;
  }

  .more-trigger {
    opacity: 0;
    transition: opacity 0.15s;
  }

  &:hover .more-trigger,
  .v-dropdown-container:has(.dropdown-menu.is-open) .more-trigger {
    opacity: 1;
  }
}

.group-items {
  padding-left: 8px;
}

.group-empty {
  padding: 8px 16px;
  font-size: 0.8rem;
  color: var(--md-sys-color-on-surface-variant);
  opacity: 0.6;
  font-style: italic;
}

// ─── Collapse transition ──────────────────────────────────────────────────
.collapse-enter-active,
.collapse-leave-active {
  transition: all 0.2s ease;
  overflow: hidden;
}

.collapse-enter-from,
.collapse-leave-to {
  max-height: 0;
  opacity: 0;
}

.collapse-enter-to,
.collapse-leave-from {
  max-height: 2000px;
  opacity: 1;
}
</style>
