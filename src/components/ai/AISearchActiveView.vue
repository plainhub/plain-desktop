<template>
  <div>
    <!-- Download progress -->
    <div v-if="isDownloading" class="active-section">
      <progress-card :label-html="progressText" :value="status.downloadProgress" />
      <v-outlined-button class="btn-block active-action" :loading="cancelDownloadLoading" @click="cancelDownload">
        {{ $t('ai.cancel_download') }}
      </v-outlined-button>
    </div>

    <!-- Loading -->
    <div v-else-if="isLoading" class="loading-row">
      <v-circular-progress indeterminate class="sm" />
      <span>{{ $t('ai.loading_subtitle') }}</span>
    </div>

    <!-- Indexing progress -->
    <div v-else-if="status.isIndexing" class="active-section">
      <p class="help-text">{{ subtitle }}</p>
      <progress-card :label-html="indexText" :value="indexProgress" />
      <v-outlined-button class="btn-block active-action" :loading="cancelIndexLoading" @click="cancelIndex">
        {{ $t('ai.stop_scan') }}
      </v-outlined-button>
    </div>

    <!-- Ready -->
    <template v-else-if="isReady">
      <div v-if="status.indexedImages === 0" class="active-section">
        <v-filled-button class="btn-block" :loading="startIndexLoading" @click="startIndex(false)">{{ $t('ai.start_index') }}</v-filled-button>
      </div>
      <div v-else class="surface-card active-data-row">
        <span>{{ $t('ai.indexed_count', { count: status.indexedImages }) }}</span>
        <v-outlined-button :loading="startIndexLoading" @click="startIndex(true)">{{ $t('ai.rescan') }}</v-outlined-button>
      </div>
      <danger-action
        class="active-danger-card"
        :label="$t('ai.unload_model')"
        :confirm-text="$t('ai.confirm_delete')"
        :loading="disableLoading"
        @confirm="disable"
      />
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { IImageSearchStatus } from '@/lib/interfaces'
import { ImageSearchStatusType } from '@/lib/status'
import { useImageSearchActions } from '@/hooks/ai/use-image-search-actions'

const props = defineProps<{ status: IImageSearchStatus }>()
const { t } = useI18n()
const { startIndex, cancelIndex, disable, cancelDownload, startIndexLoading, cancelIndexLoading, disableLoading, cancelDownloadLoading } = useImageSearchActions()

const isDownloading = computed(() => props.status.status === ImageSearchStatusType.DOWNLOADING)
const isLoading = computed(() => props.status.status === ImageSearchStatusType.LOADING)
const isReady = computed(() => props.status.status === ImageSearchStatusType.READY)

const indexProgress = computed(() => {
  if (!props.status || props.status.totalImages === 0) return 0
  return Math.round((props.status.indexedImages / props.status.totalImages) * 100)
})
const subtitle = computed(() => {
  if (props.status.isIndexing) return t('ai.scanning_subtitle')
  return t('ai.ready_subtitle')
})
const progressText = computed(() => {
  const loaded = ((props.status.downloadProgress / 100) * props.status.modelSize / 1e6).toFixed(1)
  const total = (props.status.modelSize / 1e6).toFixed(1)
  return `${t('ai.model_file')} <b>${loaded}</b> / <b>${total} MB</b>`
})
const indexText = computed(() =>
  `${t('ai.scanning_title')} <b>${props.status.indexedImages}</b> / <b>${props.status.totalImages}</b>`
)
</script>

<style lang="scss" scoped>
.active-data-row {
  display: flex; justify-content: space-between; align-items: center;
  font-size: 0.875rem; margin-bottom: 16px;
}
.active-danger-card { margin-top: 16px; }
.active-action { margin-top: 24px; }
</style>

