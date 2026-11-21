<template>
  <span v-if="item.isDir">{{ $t('x_items', item.childCount ?? 0) }}</span>
  <span v-else>{{ formatFileSize(item.size) }}</span>
  <span v-tooltip="formatDateTime(item.updatedAt)">{{ formatTimeAgo(item.updatedAt) }}</span>
</template>

<script lang="ts">
import type { IFile } from '@/lib/file'

export type FileRow = Pick<IFile, 'name' | 'isDir' | 'size' | 'updatedAt'> & { childCount?: number }
</script>

<script setup lang="ts">
import { formatFileSize, formatDateTime, formatTimeAgo } from '@/lib/format'

defineProps<{ item: FileRow }>()
</script>
