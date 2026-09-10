<template>
  <div class="item task-item" :class="`item-${stats.status}`">
    <div class="title">{{ title }}</div>
    <div class="subtitle">
      <span class="status" :class="`status-${stats.status}`">
        {{ $t(`upload_status.${stats.status}`) }}
      </span>
      <span class="size">{{ formatFileSize(stats.totalBytes) }}</span>
      <span class="count">{{ uploads.length }} {{ $t('files') }}</span>

      <div class="icon task-actions">
        <v-icon-button v-if="stats.canPause" v-tooltip="$t('pause')" class="pause-btn" @click="pauseBatch">
          <i-material-symbols:pause-rounded />
        </v-icon-button>
        <v-icon-button v-if="stats.isPausing" v-tooltip="$t('pausing')" :loading="true" class="pausing-btn" />
        <v-icon-button v-if="stats.canResume" v-tooltip="$t('resume')" class="resume-btn" @click="resumeBatch">
          <i-material-symbols:play-arrow-rounded />
        </v-icon-button>
        <v-icon-button v-if="stats.canRetry" v-tooltip="$t('retry')" class="retry-btn" @click="retryBatch">
          <i-material-symbols:refresh-rounded />
        </v-icon-button>
        <v-icon-button v-tooltip="$t('remove')" class="remove-btn" @click="removeBatch">
          <i-material-symbols:close-rounded />
        </v-icon-button>
      </div>
    </div>

    <div v-if="showProgress || stats.errorCount > 0" class="body">
      <div v-if="showProgress" class="progress-info">
        <div class="progress-text">
          {{ formatFileSize(stats.uploadedBytes) }} / {{ formatFileSize(stats.totalBytes) }} ({{ formatFileSize(liveSpeed) }}/s)
        </div>
        <div class="progress-track">
          <div class="progress-fill" :style="{ width: progressPercent + '%' }"></div>
        </div>
      </div>

      <div v-if="stats.errorCount > 0" class="error-message">
        <span v-if="stats.errorCount === 1">{{ stats.firstError }}</span>
        <span v-else>{{ stats.firstError }} (+{{ stats.errorCount - 1 }})</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { formatFileSize } from '@/lib/format'
import { useTempStore, type IUploadItem } from '@/stores/temp'
import { useI18n } from 'vue-i18n'
import { computeBatchStats, keyOf } from '@/lib/upload/batch'
import { pauseUploadsByBatch, resumeUploadsByBatch, retryUploadsByBatch, removeUploadsByBatch } from '@/lib/upload/upload-queue'

const props = defineProps<{
  batchId: string
  uploads: IUploadItem[]
}>()

const tempStore = useTempStore()
const { t } = useI18n()

const title = computed(() => `${t('upload')} (${props.uploads.length} ${t('files')})`)
const stats = computed(() => computeBatchStats(props.uploads))
const showProgress = computed(() => ['uploading', 'pending', 'saving'].includes(stats.value.status) && stats.value.uploadedBytes > 0)
const progressPercent = computed(() => (stats.value.totalBytes <= 0 ? 0 : Math.round((stats.value.uploadedBytes / stats.value.totalBytes) * 100)))

// Per-item speeds only sample after 500ms of transfer, so files that finish
// faster never report one — summing them showed 0 B/s on fast networks.
// Measure the batch's real throughput instead: diff uploadedBytes per second.
const liveSpeed = ref(0)
let speedSample: { at: number; bytes: number } | undefined
let speedTimer: ReturnType<typeof setInterval> | undefined

onMounted(() => {
  speedTimer = setInterval(() => {
    const bytes = stats.value.uploadedBytes
    const at = Date.now()
    if (speedSample) {
      const dt = (at - speedSample.at) / 1000
      if (dt > 0) liveSpeed.value = Math.max(0, Math.round((bytes - speedSample.bytes) / dt))
    }
    speedSample = { at, bytes }
  }, 1000)
})

onBeforeUnmount(() => {
  if (speedTimer) clearInterval(speedTimer)
})

function pauseBatch() {
  for (const item of pauseUploadsByBatch(props.batchId)) {
    item.pausing = true
    setTimeout(() => {
      item.pausing = false
    }, 1000)
  }
}

function resumeBatch() {
  resumeUploadsByBatch(props.batchId)
}

function retryBatch() {
  retryUploadsByBatch(props.batchId)
}

function removeBatch() {
  removeUploadsByBatch(props.batchId)
  tempStore.uploads = tempStore.uploads.filter((it) => keyOf(it) !== props.batchId)
}
</script>

<style scoped lang="scss">
@use '@/styles/task-item.scss' as *;
</style>
