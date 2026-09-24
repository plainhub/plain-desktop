<template>
  <section
    v-if="!isPhone"
    class="media-item selectable-card"
    :class="{ selected: selectedIds.includes(item.id), selecting: shiftEffectingIds.includes(item.id) }"
    @click.stop="handleItemClick($event, item, index, () => view(index))"
    @mouseenter.stop="handleMouseOver($event, index)"
  >
    <div class="start">
      <v-checkbox v-if="shiftEffectingIds.includes(item.id)" class="checkbox" touch-target="wrapper" :checked="shouldSelect" @click.stop="toggleSelect($event, item, index)" />
      <v-checkbox v-else class="checkbox" touch-target="wrapper" :checked="selectedIds.includes(item.id)" @click.stop="toggleSelect($event, item, index)" />
      <span class="number"><field-id :id="index + 1" :raw="item" /></span>
    </div>
    <div class="image">
      <img v-if="imageErrorIds.includes(item.id)" :src="`/ficons/${getFileExtension(item.path)}.svg`" class="svg" />
      <img v-else class="image-thumb" :src="getFileUrl(item.fileId, '&w=512&h=512')" loading="lazy" @error="onImageError(item.id)" />
    </div>
    <div class="title">{{ getFileName(item.path) }}</div>
    <div class="subtitle">
      <span>{{ formatFileSize(item.size) }}</span>
      <span>{{ formatDurationMs(item.durationMs) }}</span>
      <a @click.stop.prevent="viewBucket(mainStore, item.bucketId)">{{ bucketsMap[item.bucketId]?.name }}</a>
      <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
    </div>
    <VideoActionButtons
      :item="item"
      :filter="filter"
      :data-type="dataType"
      :tags="tags"
      :app="app"
      :delete-item="deleteItem"
      :restore="restore"
      :download-file="downloadFile"
      :trash="trash"
      :restore-loading="restoreLoading"
      :trash-loading="trashLoading"
    />
    <div class="time">
      <span v-tooltip="formatDateTime(item.updatedAt)">
        {{ formatTimeAgo(item.updatedAt) }}
      </span>
    </div>
  </section>

  <!-- Phone Layout -->
  <ListItemPhone
    v-else
    :is-selected="selectedIds.includes(item.id)"
    :is-selecting="shiftEffectingIds.includes(item.id)"
    :checkbox-checked="shiftEffectingIds.includes(item.id) ? shouldSelect : selectedIds.includes(item.id)"
    :show-checkbox="true"
    @click="handleItemClick($event, item, index, () => view(index))"
    @mouseenter.stop="handleMouseOver($event, index)"
    @checkbox-click="(event: MouseEvent) => toggleSelect(event, item, index)"
  >
    <template #image>
      <div class="image">
        <img v-if="imageErrorIds.includes(item.id)" :src="`/ficons/${getFileExtension(item.path)}.svg`" class="svg" />
        <img v-else class="image-thumb" :src="getFileUrl(item.fileId, '&w=512&h=512')" loading="lazy" @error="onImageError(item.id)" />
      </div>
    </template>
    
    <template #title>{{ getFileName(item.path) }}</template>
    
    <template #subtitle>
      <div class="subtitle">
        <span>{{ formatFileSize(item.size) }}</span>
        <span>{{ formatDurationMs(item.durationMs) }}</span>
        <a @click.stop.prevent="viewBucket(mainStore, item.bucketId)">{{ bucketsMap[item.bucketId]?.name }}</a>
        <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
      </div>
      <div class="subtitle">{{ formatTimeAgo(item.updatedAt) }}</div>
    </template>
    
    <template #actions>
      <VideoActionButtons
        :item="item"
        :filter="filter"
        :data-type="dataType"
        :tags="tags"
        :app="app"
        :delete-item="deleteItem"
        :restore="restore"
        :download-file="downloadFile"
        :trash="trash"
        :restore-loading="restoreLoading"
        :trash-loading="trashLoading"
      />
    </template>
  </ListItemPhone>
</template>

<script setup lang="ts">
import type { IVideoItem, IBucket, IFilter, ITag } from '@/lib/interfaces'
import { DataType } from '@/lib/data'
import { formatFileSize, formatDurationMs, formatSeconds, formatDateTime, formatTimeAgo } from '@/lib/format'
import { getFileUrl, getFileName, getFileExtension } from '@/lib/api/file'

interface Props {
  item: IVideoItem
  index: number
  selectedIds: string[]
  shiftEffectingIds: string[]
  shouldSelect: boolean
  isPhone: boolean
  imageErrorIds: string[]
  bucketsMap: Record<string, IBucket>
  filter: IFilter
  dataType: DataType
  tags: ITag[]
  mainStore: any
  app: any
  // Functions passed from parent
  handleItemClick: (event: MouseEvent, item: IVideoItem, index: number, callback: () => void) => void
  handleMouseOver: (event: MouseEvent, index: number) => void
  toggleSelect: (event: MouseEvent, item: IVideoItem, index: number) => void
  onImageError: (id: string) => void
  viewBucket: (store: any, bucketId: string) => void
  deleteItem: (dataType: DataType, item: IVideoItem) => void
  restore: (dataType: DataType, query: string) => void
  downloadFile: (path: string, fileName: string) => void
  trash: (dataType: DataType, query: string) => void
  view: (index: number) => void
  restoreLoading: (query: string) => boolean
  trashLoading: (query: string) => boolean
}

defineProps<Props>()
</script> 