<template>
  <section
    class="file-item selectable-card"
    :class="{ selected: selectedIds.includes(item.id), selecting: shiftEffectingIds.includes(item.id) }"
    @click.stop="handleItemClick($event, item, index, () => $emit('click'))"
    @mouseenter.stop="handleMouseOver($event, index)"
  >
    <div class="start">
      <v-checkbox
        v-if="shiftEffectingIds.includes(item.id)"
        class="checkbox"
        touch-target="wrapper"
        :checked="shouldSelect"
        @click.stop="toggleSelect($event, item, index)"
      />
      <v-checkbox
        v-else
        class="checkbox"
        touch-target="wrapper"
        :checked="selectedIds.includes(item.id)"
        @click.stop="toggleSelect($event, item, index)"
      />
      <span class="number"><field-id :id="index + 1" :raw="item" /></span>
    </div>

    <div class="image">
      <img v-if="thumbError && extError" class="svg" src="/ficons/default.svg" />
      <img v-else-if="!thumbError && canThumb" class="image-thumb" :src="thumbUrl" loading="lazy" @error="thumbError = true" />
      <img v-else-if="ext" :src="`/ficons/${ext}.svg`" class="svg" @error="extError = true" />
      <img v-else class="svg" src="/ficons/default.svg" />
    </div>

    <div class="title">{{ item.fileName }}</div>

    <div class="subtitle">
      <span>{{ formatFileSize(item.size) }}</span>
      <span v-tooltip="formatDateTime(item.updatedAt)">{{ formatTimeAgo(item.updatedAt) }}</span>
    </div>
    <div class="actions">
      <v-icon-button v-tooltip="$t('download')" class="sm" @click.stop="handleDownload">
        <i-material-symbols:download-rounded />
      </v-icon-button>
    </div>
  </section>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import type { IAppFile } from './useAppFilesData'
import { getAppFileFid } from './useAppFilesData'
import { formatFileSize, formatDateTime, formatTimeAgo } from '@/lib/format'
import { getFileUrl, getFileId, getFileExtension, download } from '@/lib/api/file'
import { getApiBaseUrl } from '@/lib/api/api'
import FieldId from '@/components/FieldId.vue'
import { isImage, isVideo } from '@/lib/file'

const props = defineProps<{
  item: IAppFile
  index: number
  selectedIds: string[]
  shiftEffectingIds: string[]
  shouldSelect: boolean
  handleItemClick: (event: MouseEvent, item: IAppFile, index: number, callback: () => void) => void
  handleMouseOver: (event: MouseEvent, index: number) => void
  toggleSelect: (event: MouseEvent, item: IAppFile, index: number) => void
}>()

defineEmits<{ click: [] }>()

const { urlTokenKey } = storeToRefs(useTempStore())
const thumbError = ref(false)
const extError = ref(false)

const ext = computed(() => getFileExtension(props.item.fileName))
const canThumb = computed(() => isImage(props.item.fileName) || isVideo(props.item.fileName))
const thumbUrl = computed(() => {
  const fileId = getFileId(urlTokenKey.value, `fid:${getAppFileFid(props.item)}`)
  return getFileUrl(fileId, '&w=50&h=50')
})

function handleDownload() {
  const path = `fid:${getAppFileFid(props.item)}`
  const fileId = getFileId(urlTokenKey.value, path)
  const url = `${getApiBaseUrl()}/fs?id=${encodeURIComponent(fileId)}&dl=1`
  download(url, props.item.fileName)
}
</script>

<style lang="scss" scoped>
.image-thumb {
  width: 50px;
  height: 50px;
  object-fit: cover;
  border-radius: 4px;
}
</style>
