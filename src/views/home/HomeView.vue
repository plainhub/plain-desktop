<template>
  <div class="grids">
    <template v-for="item in homeFeatureCards" :key="item.id">
      <FeatureCard v-if="item.sectionType === 'feature'" :to="item.to" :title="$t(item.titleKey)" :count="item.count">
        <template #icon>
          <component :is="item.icon" />
        </template>
        <div v-if="item.showStorageInfo && counter.total >= 0" class="storage-info">
          {{ $t('storage_free_total', { free: formatFileSize(counter.free), total: formatFileSize(counter.total) }) }}
        </div>

        <div v-if="isNas && item.showStorageInfo" class="scan-panel">
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
      </FeatureCard>

      <div v-else-if="item.sectionType === 'clipboard'" class="card clipboard-card">
        <div class="card-content">
          <h5 class="card-title">{{ $t('send_to_phone_clipboard') }}</h5>
          <div class="phone-input-row">
            <v-text-field v-model="clipText" :label="$t('clipboard_text')" class="phone-input" :error="clipTextError" :error-text="$t('valid.required')" @keyup.enter="sendClipboard">
              <template #trailing-icon>
                <v-icon-button @click.prevent="pasteClipboardText">
                  <i-material-symbols:content-paste-rounded />
                </v-icon-button>
              </template>
            </v-text-field>
            <v-filled-button class="call-btn" :loading="setClipLoading" @click.prevent="sendClipboard">
              {{ $t('send') }}
            </v-filled-button>
          </div>
        </div>
      </div>

      <CallPhoneCard v-else />
    </template>
  </div>
</template>

<script setup lang="ts">
import { formatFileSize } from '@/lib/format'
import { computed } from 'vue'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { buildQuery } from '@/lib/search'
import { encodeBase64 } from '@/lib/strutil'
import { DriveType } from '@/lib/status'
import { useHomeData, useClipboardAction, useScanAction } from './home'
import { useHomeFeatureCards } from './useHomeFeatureCards'
import CallPhoneCard from './CallPhoneCard.vue'
import FeatureCard from './FeatureCard.vue'

const { app, counter, isNas } = storeToRefs(useTempStore())

const { mounts } = useHomeData()
const { clipText, clipTextError, setClipLoading, pasteClipboardText, sendClipboard } = useClipboardAction()
const {
  scanProgress, scanActive, percent, stateLabel, counting,
  showPause, showResume, showStop, showRebuild, rebuildIndexLoading,
  pauseScan, resumeScan, stopScan, rebuildIndex,
} = useScanAction()

const filesPath = computed(() => {
  const internalRoot = mounts.value.find((m) => m.driveType === DriveType.INTERNAL_STORAGE)?.mountPoint || app.value.internalStoragePath
  const q = buildQuery([
    { name: 'parent', op: '', value: internalRoot },
    { name: 'type', op: '', value: 'INTERNAL_STORAGE' },
    { name: 'root_path', op: '', value: internalRoot },
  ])
  return `/files?q=${encodeBase64(q)}`
})

const { homeFeatureCards } = useHomeFeatureCards(filesPath)
</script>

<style lang="scss" scoped src="./HomeView.scss"></style>
