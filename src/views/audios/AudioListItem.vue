<template>
  <section
    v-if="!isPhone"
    class="media-item selectable-card"
    :class="{ selected: selectedIds.includes(item.id), selecting: shiftEffectingIds.includes(item.id) }"
    @click.stop="handleItemClick($event, item, index, () => play(item))"
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
    <div class="title">{{ item.title }}</div>
    <div class="subtitle">
      <span>{{ formatFileSize(item.size) }}</span>
      <span class="duration">
        {{ formatDurationMs(item.durationMs) }}
      </span>
      <a @click.stop.prevent="viewBucket(mainStore, item.bucketId)">{{ bucketsMap[item.bucketId]?.name }}</a>
      <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
    </div>
    <AudioActionButtons
      :item="item"
      :filter="filter"
      :data-type="dataType"
      :tags="tags"
      :animating-ids="animatingIds"
      :play-loading="playLoading"
      :play-path="playPath"
      :app="app"
      :delete-item="deleteItem"
      :restore="restore"
      :download-file="downloadFile"
      :trash="trash"
      :handle-remove-from-playlist="handleRemoveFromPlaylist"
      :add-to-playlist="addToPlaylist"
      :pause="pause"
      :is-audio-playing="isAudioPlaying"
      :is-in-playlist="isInPlaylist"
      :restore-loading="restoreLoading"
      :trash-loading="trashLoading"
    />
    <div class="artist">{{ item.artist }}</div>
    <div class="time">
      <span v-tooltip="formatDateTime(item.updatedAt)">
        {{ formatTimeAgo(item.updatedAt) }}
      </span>
    </div>
  </section>

  <!-- phone Layout -->
  <ListItemPhone
    v-else
    :is-selected="selectedIds.includes(item.id)"
    :is-selecting="shiftEffectingIds.includes(item.id)"
    :checkbox-checked="shiftEffectingIds.includes(item.id) ? shouldSelect : selectedIds.includes(item.id)"
    :show-checkbox="true"
    @click="handleItemClick($event, item, index, () => play(item))"
    @mouseenter.stop="handleMouseOver($event, index)"
    @checkbox-click="(event: MouseEvent) => toggleSelect(event, item, index)"
  >
    <template #image>
      <div class="image">
        <img v-if="imageErrorIds.includes(item.id)" :src="`/ficons/${getFileExtension(item.path)}.svg`" class="svg" />
        <img v-else class="image-thumb" :src="getFileUrl(item.fileId, '&w=512&h=512')" loading="lazy" @error="onImageError(item.id)" />
      </div>
    </template>

    <template #title>{{ item.title }}</template>

    <template #subtitle>
      <div class="subtitle">
        <span>{{ formatFileSize(item.size) }}</span>
        <span class="duration">{{ formatDurationMs(item.durationMs) }}</span>
      </div>
      <div v-if="bucketsMap[item.bucketId] || item.tags.length > 0" class="subtitle">
        <a @click.stop.prevent="viewBucket(mainStore, item.bucketId)">{{ bucketsMap[item.bucketId]?.name }}</a>
        <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
      </div>
      <div class="subtitle">
        <span v-if="item.artist" class="artist">{{ item.artist }}</span>
        <span class="time">{{ formatTimeAgo(item.createdAt) }}</span>
      </div>
    </template>

    <template #actions>
      <AudioActionButtons
        :item="item"
        :filter="filter"
        :data-type="dataType"
        :tags="tags"
        :animating-ids="animatingIds"
        :play-loading="playLoading"
        :play-path="playPath"
        :app="app"
        :delete-item="deleteItem"
        :restore="restore"
        :download-file="downloadFile"
        :trash="trash"
        :handle-remove-from-playlist="handleRemoveFromPlaylist"
        :add-to-playlist="addToPlaylist"
        :pause="pause"
        :is-audio-playing="isAudioPlaying"
        :is-in-playlist="isInPlaylist"
        :restore-loading="restoreLoading"
        :trash-loading="trashLoading"
      />
    </template>
  </ListItemPhone>
</template>

<script setup lang="ts">
import type { IAudioWithFileId, IBucket, IFilter, ITag } from '@/lib/interfaces'
import { DataType } from '@/lib/data'
import { formatFileSize, formatDurationMs, formatSeconds, formatDateTime, formatTimeAgo } from '@/lib/format'
import { getFileUrl, getFileExtension } from '@/lib/api/file'

interface Props {
  item: IAudioWithFileId
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
  animatingIds: string[]
  playLoading: boolean
  playPath: string
  mainStore: any
  app: any
  // Functions passed from parent
  handleItemClick: (event: MouseEvent, item: IAudioWithFileId, index: number, callback: () => void) => void
  handleMouseOver: (event: MouseEvent, index: number) => void
  toggleSelect: (event: MouseEvent, item: IAudioWithFileId, index: number) => void
  onImageError: (id: string) => void
  viewBucket: (store: any, bucketId: string) => void
  deleteItem: (dataType: DataType, item: IAudioWithFileId) => void
  restore: (dataType: DataType, query: string) => void
  downloadFile: (path: string, fileName: string) => void
  trash: (dataType: DataType, query: string) => void
  handleRemoveFromPlaylist: (event: MouseEvent, item: IAudioWithFileId) => void
  addToPlaylist: (event: MouseEvent, item: IAudioWithFileId) => void
  play: (item: IAudioWithFileId) => void
  pause: () => void
  isAudioPlaying: (item: IAudioWithFileId) => boolean
  isInPlaylist: (item: IAudioWithFileId) => boolean
  restoreLoading: (query: string) => boolean
  trashLoading: (query: string) => boolean
}

defineProps<Props>()
</script>
