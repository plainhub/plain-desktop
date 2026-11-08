<template>
  <section
    class="doc-item selectable-card"
    :class="{ selected: selectedIds.includes(item.id), selecting: shiftEffectingIds.includes(item.id) }"
    @click.stop="handleItemClick($event, item, index, () => $emit('open-file', item))"
    @mouseenter.stop="handleMouseOver($event, index)"
  >
    <div class="start">
      <v-checkbox v-if="shiftEffectingIds.includes(item.id)" class="checkbox" touch-target="wrapper" :checked="shouldSelect" @click.stop="toggleSelect($event, item, index)" />
      <v-checkbox v-else class="checkbox" touch-target="wrapper" :checked="selectedIds.includes(item.id)" @click.stop="toggleSelect($event, item, index)" />
      <span class="number"><field-id :id="index + 1" :raw="item" /></span>
    </div>

    <div class="doc-icon">
      <img :src="`/ficons/${item.extension}.svg`" class="svg" @error="onIconError" />
    </div>

    <div class="title">
      {{ item.title }}
      <OnlinePreviewIcon :name="getFileName(item.path)" />
    </div>

    <div class="subtitle">
      <span>{{ formatFileSize(item.size) }}</span>
      <span v-tooltip="formatDateTimeFull(item.updatedAt)">{{ formatTimeAgo(item.updatedAt) }}</span>
      <a v-if="bucketsMap[item.bucketId]" @click.stop.prevent="viewBucket(mainStore, item.bucketId)">{{ bucketsMap[item.bucketId]?.name }}</a>
      <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
    </div>

    <DocActionButtons
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
      @delete-item="$emit('delete-item', $event)"
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

defineProps<{
  item: IDoc
  index: number
  selectedIds: string[]
  shiftEffectingIds: string[]
  shouldSelect: boolean
  filter: IFilter
  app: IApp
  dataType: DataType
  tags: ITag[]
  trashLoading: (query: string) => boolean
  restoreLoading: (query: string) => boolean
  trash: (dataType: DataType, query: string) => void
  restore: (dataType: DataType, query: string) => void
  deleteItem: (dataType: DataType, item: any) => void
  bucketsMap: Record<string, IBucket>
  viewBucket: (store: any, bucketId: string) => void
  handleItemClick: (event: MouseEvent, item: IDoc, index: number, callback: () => void) => void
  handleMouseOver: (event: MouseEvent, index: number) => void
  toggleSelect: (event: MouseEvent, item: IDoc, index: number) => void
}>()

defineEmits<{
  'download-file': [path: string]
  'delete-item': [item: IDoc]
  'open-file': [item: IDoc]
  'rename-item': [item: IDoc]
  'duplicate-item': [item: IDoc]
}>()

function onIconError(e: Event) {
  const img = e.target as HTMLImageElement
  img.src = '/ficons/default.svg'
}
</script>


