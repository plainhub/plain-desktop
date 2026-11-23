<template>
  <section
    class="doc-item selectable-card"
    :class="{ 'read-only': readOnly, selected: selectedIds.includes(item.id), selecting: shiftEffectingIds.includes(item.id) }"
    :role="readOnly ? 'link' : undefined"
    :tabindex="readOnly ? 0 : undefined"
    @click.stop="handleItemClick ? handleItemClick($event, item, index, () => $emit('open-file', item)) : $emit('open-file', item)"
    @keydown.enter="readOnly && $emit('open-file', item)"
    @mouseenter.stop="handleMouseOver?.($event, index)"
  >
    <div v-if="!readOnly" class="start">
      <v-checkbox v-if="shiftEffectingIds.includes(item.id)" class="checkbox" touch-target="wrapper" :checked="shouldSelect" @click.stop="toggleSelect?.($event, item, index)" />
      <v-checkbox v-else class="checkbox" touch-target="wrapper" :checked="selectedIds.includes(item.id)" @click.stop="toggleSelect?.($event, item, index)" />
      <span class="number"><field-id :id="index + 1" :raw="item" /></span>
    </div>

    <div class="doc-icon">
      <img :src="`/ficons/${item.extension}.svg`" class="svg" @error="onIconError" />
    </div>

    <div class="title">
      {{ item.title || getFileName(item.path) }}
      <OnlinePreviewIcon :name="getFileName(item.path)" />
    </div>

    <div class="subtitle">
      <span>{{ formatFileSize(item.size) }}</span>
      <span v-tooltip="formatDateTimeFull(item.updatedAt)">{{ formatTimeAgo(item.updatedAt) }}</span>
      <span v-if="readOnly" class="doc-path">{{ item.path }}</span>
      <template v-else>
        <a v-if="bucketsMap?.[item.bucketId]" @click.stop.prevent="viewBucket?.(mainStore, item.bucketId)">{{ bucketsMap[item.bucketId]?.name }}</a>
        <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
      </template>
    </div>

    <DocActionButtons
      v-if="!readOnly && filter && app && trashLoading && restoreLoading && trash && restore && deleteItem"
      :item="item"
      :filter="filter"
      :app="app"
      :data-type="dataType"
      :tags="tags"
      :trash-loading="trashLoading"
      :restore-loading="restoreLoading"
      :trash="trash"
      :restore="restore"
      :delete-item="deleteItem"
      @download-file="$emit('download-file', $event)"
      @open-file="$emit('open-file', $event)"
      @rename-item="$emit('rename-item', $event)"
      @duplicate-item="$emit('duplicate-item', $event)"
    />
  </section>
</template>

<script setup lang="ts">
import type { IDoc, IFilter, IApp, IBucket, ITag } from '@/lib/interfaces'
import { DataType } from '@/lib/data'
import { formatFileSize, formatDateTimeFull, formatTimeAgo } from '@/lib/format'
import { getFileName } from '@/lib/api/file'
import DocActionButtons from './DocActionButtons.vue'
import OnlinePreviewIcon from '@/components/OnlinePreviewIcon.vue'
import { useMainStore } from '@/stores/main'

const mainStore = useMainStore()

withDefaults(defineProps<{
  item: IDoc
  index: number
  readOnly?: boolean
  selectedIds?: string[]
  shiftEffectingIds?: string[]
  shouldSelect?: boolean
  filter?: IFilter
  app?: IApp
  dataType?: DataType
  tags?: ITag[]
  trashLoading?: (query: string) => boolean
  restoreLoading?: (query: string) => boolean
  trash?: (dataType: DataType, query: string) => void
  restore?: (dataType: DataType, query: string) => void
  deleteItem?: (item: IDoc) => Promise<boolean>
  bucketsMap?: Record<string, IBucket>
  viewBucket?: (store: any, bucketId: string) => void
  handleItemClick?: (event: MouseEvent, item: IDoc, index: number, callback: () => void) => void
  handleMouseOver?: (event: MouseEvent, index: number) => void
  toggleSelect?: (event: MouseEvent, item: IDoc, index: number) => void
}>(), {
  selectedIds: () => [],
  shiftEffectingIds: () => [],
  tags: () => [],
  dataType: DataType.DOC,
})

defineEmits<{
  'download-file': [path: string]
  'open-file': [item: IDoc]
  'rename-item': [item: IDoc]
  'duplicate-item': [item: IDoc]
}>()

function onIconError(e: Event) {
  const img = e.target as HTMLImageElement
  img.src = '/ficons/default.svg'
}
</script>

<style lang="scss">
.doc-item {
  grid-template-areas:
    'start icon title actions'
    'start icon subtitle actions';
  grid-template-columns: 48px 40px 1fr auto;
  align-items: center;

  &.read-only {
    grid-template-areas: 'icon title' 'icon subtitle';
    grid-template-columns: 40px minmax(0, 1fr);
    padding-inline: 12px;
  }

  .doc-path {
    overflow-wrap: anywhere;
  }

  &:hover { cursor: pointer; }

  .doc-icon {
    grid-area: icon;
    display: flex;
    align-items: center;
    margin-block: 10px 8px;
    .svg { width: 32px; height: 32px; object-fit: contain; }
  }

  .title {
    grid-area: title;
    padding-block-start: 8px;
  }

  .subtitle {
    grid-area: subtitle;
    display: flex;
    gap: 12px;
    color: var(--md-sys-color-secondary);
    font-size: 0.85rem;
    margin-block-end: 8px;
  }
}
</style>
