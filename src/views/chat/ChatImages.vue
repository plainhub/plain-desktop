<template>
  <div class="image-container">
    <div v-for="(item, i) in sources" :key="i" class="media-item" data-ctx="media" :data-path="item.path" :data-name="item.name" @click="canView ? view(i) : undefined">
      <img v-if="getPreview(item)" class="image-thumb" :src="getPreview(item)" loading="lazy" onerror="this.src='/broken-image.png'" />
      <span class="duration">{{ isVideo(item.name) && item.duration > 0 ? formatSeconds(item.duration) : formatFileSize(item.size) }}</span>
      <ChatDownloadOverlay :download-info="downloadInfo" :ring-size="48" border-radius="6px" @action="onDownloadAction" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { getFileName, getFileUrl, notId, getPeerProxyUrl } from '@/lib/api/file'
import { isVideo } from '@/lib/file'
import { computed } from 'vue'
import type { ISource } from '@/components/lightbox/types'
import { formatSeconds, formatFileSize } from '@/lib/format'
import { useTempStore } from '@/stores/temp'
import { useOpenMedia } from '@/hooks/open-media'
import ChatDownloadOverlay from './ChatDownloadOverlay.vue'

const tempStore = useTempStore()
const props = defineProps({
  data: { type: Object, required: true },
  downloadInfo: { type: Object as () => { downloaded: number; total: number; speed: number; status: string } | null, default: null },
  peer: { type: Object as () => { ip: string; port: number } | null, default: null },
})

const emit = defineEmits<{ 'download-action': [action: 'pause' | 'resume' | 'retry'] }>()
function onDownloadAction(action: 'pause' | 'resume' | 'retry') {
  emit('download-action', action)
}

const ACTIVE_STATUSES = ['PENDING', 'DOWNLOADING', 'PAUSED', 'FAILED']
function isActiveStatus(status: string) {
  return ACTIVE_STATUSES.includes(status)
}

const canView = computed(() => !props.downloadInfo || !isActiveStatus(props.downloadInfo.status))

function getPreview(source: ISource) {
  if (source.thumbnail) {
    return source.thumbnail
  }

  // Received from a peer: the URI is `fsid:<peer-encrypted-id>` and the
  // file lives on the peer. The local /fs endpoint cannot resolve `fsid:`,
  // so always route peer files through /proxyfs (which forwards to the
  // peer's /fs), not just while a download is in progress.
  if (props.peer && source.path.startsWith('fsid:')) {
    return getPeerProxyUrl(tempStore.urlTokenKey, props.peer, source.path.slice(4), '&w=512&h=512')
  }

  if (props.downloadInfo && isActiveStatus(props.downloadInfo.status)) {
    return ''
  }

  if (source.src.startsWith('blob:')) {
    return source.src
  }

  return `${source.src}&w=512&h=512`
}

const sources = computed(() => {
  const data = props.data
  const files = data?._content?.value?.items ?? []
  const peer = props.peer
  const items: ISource[] = []
  data?.data?.ids?.forEach((id: string, index: number) => {
    const file = files[index]
    const uri = file.uri
    const peerFileId = typeof uri === 'string' && uri.startsWith('fsid:') ? uri.slice(4) : ''
    const isGif = typeof uri === 'string' && uri.endsWith('.gif')
    // Received from a peer (`fsid:`): the file lives on the peer, so route
    // through /proxyfs. Bake the resize/conversion query into the encrypted
    // id — /proxyfs ignores query params on the outer URL, and the peer's /fs
    // needs `w/h` to convert HEIC (Pixel) into a browser-displayable JPEG.
    // GIFs keep the original (no resize) to preserve animation.
    const src = peer && peerFileId
      ? getPeerProxyUrl(tempStore.urlTokenKey, peer, peerFileId, isGif ? '' : '&w=1024&h=1024&cc=false')
      : getFileUrl(id)
    items.push({
      path: file.uri,
      src,
      viewOriginImage: notId(id) || isGif,
      name: file.fileName || getFileName(file.uri),
      duration: file.duration,
      size: file.size,
      thumbnail: file.thumbnail,
      isFromChat: true,
    })
  })

  return items
})

const { open: openMedia } = useOpenMedia(sources)
function view(index: number) { openMedia(index) }
</script>

