<template>
  <div class="bookmark-item" @click="$emit('click')">
    <!-- Favicon -->
    <div class="favicon-wrap">
      <img
        v-if="faviconUrl"
        :src="faviconUrl"
        class="favicon"
        @error="faviconError = true"
      />
      <i-lucide:link v-else class="favicon-placeholder" />
    </div>

    <!-- Content -->
    <div class="bookmark-content">
      <div class="bookmark-title">{{ bookmark.title || bookmark.url }}</div>
      <div class="bookmark-url">{{ displayDomain }}</div>
    </div>

    <v-dropdown v-model="menuVisible" :class="{ 'menu-open': menuVisible }" @click.stop>
      <template #trigger>
        <v-icon-button class="icon more-trigger">
          <i-material-symbols:more-horiz />
        </v-icon-button>
      </template>
      <div
        class="dropdown-item"
        :class="{ selected: bookmark.pinned }"
        @click="$emit('toggle-pin'); menuVisible = false"
      >
        {{ bookmark.pinned ? $t('unpin') : $t('pin') }}
      </div>
      <div class="dropdown-item" @click="$emit('edit'); menuVisible = false">
        {{ $t('edit') }}
      </div>
      <div class="dropdown-item danger" @click="$emit('delete'); menuVisible = false">
        {{ $t('delete') }}
      </div>
    </v-dropdown>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { getFileUrl } from '@/lib/api/file'
import { useTempStore } from '@/stores/temp'
import type { Bookmark, BookmarkGroup } from '@/stores/bookmarks'
import { getFileId } from '@/lib/api/file'
import { storeToRefs } from 'pinia'

const props = defineProps<{
  bookmark: Bookmark
  groups: BookmarkGroup[]
}>()

defineEmits<{
  click: []
  edit: []
  delete: []
  'toggle-pin': []
}>()

const tempStore = useTempStore()
const { urlTokenKey } = storeToRefs(tempStore)
const faviconError = ref(false)
const menuVisible = ref(false)

const faviconUrl = computed(() => {
  if (faviconError.value || !props.bookmark.faviconPath) return null
  const path = props.bookmark.faviconPath
  if (path.startsWith('http://') || path.startsWith('https://')) return path
  // app:// path — use encrypted file ID
  const id = getFileId(urlTokenKey.value, path)
  return id ? getFileUrl(id) : null
})

const displayDomain = computed(() => {
  try {
    return new URL(props.bookmark.url).hostname
  } catch {
    return props.bookmark.url
  }
})
</script>

<style scoped lang="scss">
.bookmark-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  cursor: pointer;
  position: relative;

  &:hover {
    background: var(--md-sys-color-surface-container-high);

    .more-trigger {
      opacity: 1;
    }
  }
}

.more-trigger {
  opacity: 0;
  transition: opacity 0.15s;
}

.menu-open .more-trigger {
  opacity: 1;
}

.favicon-wrap {
  width: 24px;
  height: 24px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;

  .favicon {
    width: 20px;
    height: 20px;
    object-fit: contain;
    border-radius: 3px;
  }

  .favicon-placeholder {
    width: 18px;
    height: 18px;
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.5;
  }
}

.bookmark-content {
  flex: 1;
  min-width: 0;

  .bookmark-title {
    font-size: 0.875rem;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--md-sys-color-on-surface);
  }

  .bookmark-url {
    font-size: 0.75rem;
    color: var(--md-sys-color-on-surface-variant);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    opacity: 0.7;
  }
}

</style>
