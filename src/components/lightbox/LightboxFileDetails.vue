<template>
  <div class="file-details">
    <LightboxFileInfoItem :label="$t('file_size')">
      {{ formatFileSize(current?.size ?? 0) }}
      <span v-if="fileInfo?.data?.width && fileInfo?.data?.height" class="info-resolution">{{ getResolution() }}</span>
    </LightboxFileInfoItem>
    
    <LightboxFileInfoItem 
      v-if="fileInfo?.updatedAt" 
      :label="$t('updated_at')"
    >
      <time v-tooltip="formatDateTimeFull(fileInfo.updatedAt)">{{ formatDateTime(fileInfo.updatedAt) }}</time>
    </LightboxFileInfoItem>
    
    <LightboxFileInfoItem 
      v-if="current && (isAudio(current?.name || '') || isVideo(current?.name || ''))" 
      :label="$t('duration')"
      :value="formatDurationMs(fileInfo?.data?.durationMs ?? current?.durationMs)"
    />
    
    <LightboxFileInfoItem 
      v-if="current?.path" 
      :label="$t('path')" 
      :value="getFinalPath(appDir, current.path)" 
      is-path
    />
  </div>
</template>

<script setup lang="ts">
import { formatFileSize, formatDurationMs, formatSeconds, formatDateTime, formatDateTimeFull } from '@/lib/format'
import { isVideo, isImage, isAudio } from '@/lib/file'
import { getFinalPath } from '@/lib/api/file'
import type { ISource } from './types'

const props = defineProps({
  current: {
    type: Object as () => ISource | undefined,
    required: true,
  },
  fileInfo: {
    type: Object,
    default: null,
  },
  appDir: {
    type: String,
    default: '',
  },
})

function getResolution() {
  const width = props.fileInfo?.data?.width ?? 0
  const height = props.fileInfo?.data?.height ?? 0
  let r = `  ${width} x ${height}`
  if (isImage(props.current?.name ?? '')) {
    const mp = Math.round((width * height) / 1000000)
    if (mp > 1) {
      r += `  ${mp} MP`
    }
  }
  return r
}
</script>

<style lang="scss" scoped>
.info-resolution {
  margin-inline-start: 4px;
  color: var(--md-sys-color-on-surface-variant);
}
</style> 