<template>
  <div v-if="isActive" class="download-overlay" :style="{ borderRadius }" @click.stop="onClick">
    <svg
      class="progress-ring"
      :class="{ spinning: downloadInfo?.status === 'PENDING' }"
      :width="ringSize"
      :height="ringSize"
      :viewBox="`0 0 ${viewBox} ${viewBox}`"
    >
      <circle class="ring-track" :cx="center" :cy="center" :r="radius" />
      <circle
        v-if="downloadInfo?.status !== 'PENDING'"
        class="ring-progress"
        :cx="center"
        :cy="center"
        :r="radius"
        :style="{ strokeDasharray: circumference, strokeDashoffset: dashOffset }"
      />
    </svg>
    <span class="overlay-icon" :style="{ fontSize: iconSize + 'px' }">
      <i-material-symbols:pause-rounded v-if="downloadInfo?.status === 'DOWNLOADING'" />
      <i-material-symbols:download-rounded v-else-if="downloadInfo?.status === 'PAUSED'" />
      <i-material-symbols:close-rounded v-else-if="downloadInfo?.status === 'PENDING'" />
      <i-material-symbols:refresh-rounded v-else-if="downloadInfo?.status === 'FAILED' || downloadInfo?.status === 'PARTIAL' || downloadInfo?.status === 'CANCELED'" />
    </span>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

interface DownloadInfo {
  downloaded: number
  total: number
  speed: number
  status: string
}

const props = withDefaults(
  defineProps<{
    downloadInfo?: DownloadInfo | null
    /** outer size of the SVG ring in px */
    ringSize?: number
    /** border-radius forwarded to the backdrop */
    borderRadius?: string
  }>(),
  {
    downloadInfo: null,
    ringSize: 52,
    borderRadius: '0px',
  }
)

const emit = defineEmits<{ action: [action: 'pause' | 'resume' | 'retry'] }>()

const ACTIVE_STATUSES = ['PENDING', 'DOWNLOADING', 'PAUSED', 'FAILED', 'PARTIAL', 'CANCELED']

const isActive = computed(
  () => !!props.downloadInfo && ACTIVE_STATUSES.includes(props.downloadInfo.status)
)

// Mirrors plain-app DownloadActionButton: the icon doubles as the control.
// downloading -> pause, paused -> resume, pending -> pause, failed -> retry.
function onClick() {
  const s = props.downloadInfo?.status
  if (s === 'DOWNLOADING' || s === 'PENDING') emit('action', 'pause')
  else if (s === 'PAUSED') emit('action', 'resume')
  else if (s === 'FAILED' || s === 'PARTIAL' || s === 'CANCELED') emit('action', 'retry')
}

// SVG geometry derived from ringSize: use 80 % of ringSize as viewBox, radius = 40 % of viewBox
const viewBox = computed(() => props.ringSize)
const center = computed(() => props.ringSize / 2)
const radius = computed(() => props.ringSize * 0.4)
const circumference = computed(() => +(2 * Math.PI * radius.value).toFixed(2))
const iconSize = computed(() => Math.round(props.ringSize * 0.38))

const progressPercent = computed(() => {
  const d = props.downloadInfo
  if (!d || d.total <= 0) return 0
  return Math.min(100, d.downloaded / d.total)
})

const dashOffset = computed(() => {
  const c = circumference.value
  return +(c - progressPercent.value * c).toFixed(2)
})
</script>

<style lang="scss" scoped>
.download-overlay {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.45);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}

.progress-ring {
  position: absolute;

  .ring-track {
    fill: none;
    stroke: rgba(255, 255, 255, 0.3);
    stroke-width: 3;
  }

  .ring-progress {
    fill: none;
    stroke: #fff;
    stroke-width: 3;
    stroke-linecap: round;
    transform: rotate(-90deg);
    transform-origin: 50% 50%;
    transition: stroke-dashoffset 0.5s ease;
  }

  &.spinning {
    animation: ring-spin 1s linear infinite;

    .ring-track {
      stroke-dasharray: v-bind('circumference * 0.7 + " " + circumference * 0.3');
    }
  }
}

.overlay-icon {
  position: absolute;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
}

@keyframes ring-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
