<template>
  <div class="scan-panel">
    <template v-if="scanActive">
      <div class="scan-row">
        <span v-if="stateLabel">{{ stateLabel }}</span>
        <span v-if="percent > 0" class="muted">{{ percent }}%</span>
      </div>
      <div class="progress">
        <div class="bar" :style="{ width: percent + '%' }"></div>
      </div>
      <div v-if="!counting" class="muted">
        {{ scanProgress.indexed.toLocaleString() }} / {{ scanProgress.total.toLocaleString() }}
        <span v-if="scanProgress.pending > 0"> · {{ $t('pending') }} {{ scanProgress.pending.toLocaleString() }}</span>
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
  scanProgress, scanActive, percent, stateLabel, counting,
  showPause, showResume, showStop, showRebuild, rebuildIndexLoading,
  pauseScan, resumeScan, stopScan, rebuildIndex,
} = useScanAction()
</script>

<style scoped lang="scss">
.scan-panel {
  width: 100%;
  max-width: 320px;
  margin-top: 10px;

  .scan-row {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
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
    margin-top: 10px;
    flex-wrap: wrap;
  }
}
</style>
