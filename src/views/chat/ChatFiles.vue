<template>
  <div class="file-container">
    <div v-for="(item, i) in items" :key="i" class="file-item-wrapper">
      <div class="file-item" data-ctx="media" :data-path="item.path" :data-name="item.name" @click="clickItem(item)">
        <div class="file-content">
          <div class="file-name" :class="{ playing: activeAudioSrc === item.src }">{{ item.name }}</div>
          <div class="file-info">
            {{ formatFileSize(item.size) }}{{ item.duration > 0 ? ' / ' + formatSeconds(item.duration) : '' }}
          </div>
          <div v-if="item.summary" class="file-summary">{{ item.summary }}</div>
        </div>
        <div class="thumb-wrap">
          <img v-if="getThumb(item)" :src="getThumb(item)" class="file-thumbnail" :class="{ 'file-icon': !isImage(item.name) && !isVideo(item.name) }" loading="lazy" decoding="async" @error="onIconError(item.name)" />
          <ChatDownloadOverlay :download-info="downloadInfo" :ring-size="40" border-radius="8px" @action="onDownloadAction" />
        </div>
      </div>
      <ChatAudioPlayer v-if="activeAudioSrc === item.src" :src="item.src" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { isVideo, isImage } from '@/lib/file'
import { formatSeconds, formatFileSize } from '@/lib/format'
import ChatAudioPlayer from './ChatAudioPlayer.vue'
import ChatDownloadOverlay from './ChatDownloadOverlay.vue'
import { useChatFiles } from './hooks/chat-files'

const props = defineProps({
  data: { type: Object, default: () => ({}) },
  downloadInfo: { type: Object as () => { downloaded: number; total: number; speed: number; status: string } | null, default: null },
  peer: { type: Object as () => { ip: string; port: number } | null, default: null },
})

const emit = defineEmits<{ 'download-action': [action: 'pause' | 'resume' | 'retry'] }>()
function onDownloadAction(action: 'pause' | 'resume' | 'retry') {
  emit('download-action', action)
}

const { items, activeAudioSrc, getThumb, onIconError, clickItem } = useChatFiles(props)
</script>

<style lang="scss" scoped>
.file-container {
  margin-top: 6px;
  max-width: 600px;
}

.file-item-wrapper {
  margin-bottom: 6px;
  background: var(--md-sys-color-surface-container);
  border-radius: 12px;
  overflow: hidden;
  &:last-child { margin-bottom: 0; }
}

.file-item {
  display: flex;
  align-items: center;
  padding: 12px 16px;
  cursor: pointer;
  transition: background 0.15s;
  &:hover { background: var(--md-sys-color-surface-container-high); }
}

.file-content {
  flex: 1;
  min-width: 0;
}

.file-name {
  font-weight: 500;
  color: var(--md-sys-color-on-surface);
  word-break: break-all;
  margin-bottom: 4px;
  &.playing { color: var(--md-sys-color-on-surface-variant); }
}

.file-info {
  font-size: 0.875rem;
  color: var(--md-sys-color-on-surface-variant);
}

.file-summary {
  font-size: 0.8rem;
  color: var(--md-sys-color-on-surface-variant);
  margin-top: 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}

.file-thumbnail {
  width: 48px;
  height: 48px;
  border-radius: 8px;
  object-fit: cover;
  display: block;
}

.thumb-wrap {
  position: relative;
  width: 48px;
  height: 48px;
  margin-left: 12px;
  flex-shrink: 0;
  background: var(--md-sys-color-surface-container-high);
  border-radius: 8px;
}

.file-icon {
  object-fit: contain;
  border-radius: 0;
  background: none;
}

// Overlay styles handled by ChatDownloadOverlay component
</style>
