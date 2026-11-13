<template>
  <div class="image-container">
    <div v-for="(item, i) in sources" :key="i" class="media-item" data-ctx="media" :data-path="item.path" :data-name="item.name" @click="view(item)">
      <img v-if="getPreview(item)" class="image-thumb" :src="getPreview(item)" loading="lazy" onerror="this.src='/broken-image.png'" />
      <span class="duration">{{ isVideo(item.name) && item.durationMs > 0 ? formatDurationMs(item.durationMs) : formatFileSize(item.size) }}</span>
      <ChatDownloadOverlay v-if="item.path.startsWith('fsid:')" :download-info="downloadInfo" :ring-size="48" border-radius="6px" @action="onDownloadAction" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { chatAttachmentThumbnail } from '@/lib/api/file-action'
import { getFileName, getFileUrl, getChatAttachmentUrl, getChatFileId } from '@/lib/api/file'
import { isVideo } from '@/lib/file'
import { computed } from 'vue'
import type { ISource } from '@/components/lightbox/types'
import { formatDurationMs, formatFileSize } from '@/lib/format'
import { useTempStore } from '@/stores/temp'
import { useOpenMedia } from '@/hooks/open-media'
import { chatFileDurationMs } from './chat-file-duration'
import ChatDownloadOverlay from './ChatDownloadOverlay.vue'

const tempStore = useTempStore()
const props = defineProps({
  data: { type: Object, required: true },
  downloadInfo: { type: Object as () => { downloaded: number; total: number; speed: number; status: string } | null, default: null },
  peer: { type: Object as () => { id: string } | null, default: null },
})

const emit = defineEmits<{ 'download-action': [action: 'pause' | 'resume' | 'retry'] }>()
function onDownloadAction(action: 'pause' | 'resume' | 'retry') {
  emit('download-action', action)
}

function getPreview(source: ISource) {
  if (source.thumbnail) {
    return source.thumbnail
  }

  if (source.path.startsWith('fsid:')) return chatAttachmentThumbnail(source.src)

  if (source.src.startsWith('blob:')) {
    return source.src
  }

  return `${source.src}&size=sm`
}

const sources = computed(() => {
  const data = props.data
  const files = data?._content?.value?.items ?? []
  const peer = props.peer
  const items: ISource[] = []
  files.forEach((file: any) => {
    const uri = file.uri
    const remote = typeof uri === 'string' && uri.startsWith('fsid:')
    const isGif = typeof uri === 'string' && uri.endsWith('.gif')
    const src = peer && remote
      ? getChatAttachmentUrl(tempStore.urlTokenKey, peer.id, uri, isGif ? '' : '&size=lg')
      : getFileUrl(getChatFileId(tempStore.urlTokenKey, uri))
    items.push({
      path: file.uri,
      src,
      viewOriginImage: isGif,
      name: file.fileName || getFileName(file.uri),
      durationMs: chatFileDurationMs(file),
      size: file.size,
      thumbnail: file.thumbnail,
      isFromChat: true,
    })
  })

  return items
})

const viewable = computed(() => sources.value.filter(item => !item.path.startsWith('fsid:')))
const { open: openMedia } = useOpenMedia(viewable)
function view(item: ISource) {
  const index = viewable.value.indexOf(item)
  if (index >= 0) openMedia(index)
}
</script>

