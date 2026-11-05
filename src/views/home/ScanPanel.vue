<template>
  <div class="scan-panel">
    <div v-if="stateLabel" class="scan-row">
      <span>{{ stateLabel }}</span>
      <span v-if="scanActive && scanProgress.total !== null && scanProgress.total > 0" class="muted">{{ percent }}%</span>
    </div>
    <template v-if="scanActive">
      <div class="progress">
        <div class="bar" :style="{ width: percent + '%' }"></div>
      </div>
      <div v-if="scanProgress.total !== null || scanProgress.indexed > 0" class="muted">
        {{ scanProgress.indexed.toLocaleString() }}<template v-if="scanProgress.total !== null"> / {{ scanProgress.total.toLocaleString() }}</template>
        <span v-if="scanProgress.pending !== null && scanProgress.pending > 0"> · {{ $t('pending') }} {{ scanProgress.pending.toLocaleString() }}</span>
      </div>
    </template>

    <div class="action-row">
      <v-filled-button v-if="showPause" @click.stop.prevent="pauseScan">{{ $t('pause') }}</v-filled-button>
      <v-filled-button v-if="showResume" @click.stop.prevent="resumeScan">{{ $t('resume') }}</v-filled-button>
      <v-outlined-button v-if="showStop" @click.stop.prevent="stopScan">{{ $t('stop') }}</v-outlined-button>
      <v-outlined-button
        v-if="showRebuild || rebuildIndexLoading" :loading="rebuildIndexLoading"
        @click.stop.prevent="rebuildIndex"
      >{{ $t('rebuild_index') }}</v-outlined-button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useScanAction } from './home'

const {
  scanProgress, scanActive, percent, stateLabel,
  showPause, showResume, showStop, showRebuild, rebuildIndexLoading,
  pauseScan, resumeScan, stopScan, rebuildIndex,
} = useScanAction()
</script>

<style scoped lang="scss">
.scan-panel {
  width: 100%;
  max-width: 320px;
  margin-top: 8px;

  .scan-row {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .muted {
    margin-top: 8px;
    color: var(--md-sys-color-on-surface-variant);
    font-size: 0.875rem;
  }

  .progress {
    position: relative;
    height: 6px;
    background: rgba(0, 0, 0, 0.08);
    border-radius: 999px;
    overflow: hidden;

    .bar {
      position: absolute;
      left: 0;
      top: 0;
      bottom: 0;
      width: 0;
      background: var(--md-sys-color-primary);
      border-radius: 999px;
      transition: width 0.25s ease;
    }
  }

  .action-row {
    display: flex;
    justify-content: center;
    gap: 8px;
    margin-top: 8px;
    flex-wrap: wrap;
  }
}
</style>
