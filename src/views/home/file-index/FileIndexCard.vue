<template>
  <section class="card file-index-card">
    <header>
      <h2>{{ $t('file_index.title') }}</h2>
      <div class="actions">
        <v-icon-button v-if="showPause" v-tooltip="$t('pause')" :aria-label="$t('pause')" @click="pauseScan"><i-material-symbols:pause-rounded /></v-icon-button>
        <v-icon-button v-if="showResume" v-tooltip="$t('resume')" :aria-label="$t('resume')" @click="resumeScan"><i-material-symbols:play-arrow-rounded /></v-icon-button>
        <v-icon-button v-if="showStop" v-tooltip="$t('stop')" :aria-label="$t('stop')" @click="stopScan"><i-material-symbols:stop-rounded /></v-icon-button>
        <v-text-button v-if="volumes.length > 1 && (rebuildIndexLoading || (hasIndexScope && showRebuild))" :loading="rebuildIndexLoading" :disabled="rebuildVolumeLoading" @click="rebuildIndex">{{
          $t('rebuild_index')
        }}</v-text-button>
      </div>
    </header>
    <div v-if="stateLabel" class="summary" aria-live="polite">
      <span
        >{{ stateLabel }}<template v-if="scanActive && scanProgress.total !== null && scanProgress.total > 0"> · {{ percent }}%</template></span
      >
      <span v-if="scanProgress.indexed > 0 || (scanProgress.total !== null && scanProgress.total > 0)"
        >{{ scanProgress.indexed.toLocaleString() }}<template v-if="scanProgress.total !== null && scanProgress.total > 0"> / {{ scanProgress.total.toLocaleString() }}</template></span
      >
    </div>
    <div v-if="volumes.length" class="volume-cards">
      <FileIndexVolumeCard
        v-for="volume in volumes"
        :key="volume.id"
        :volume="volume"
        :title="volumeName(volume)"
        :mount="mounts.find((mount) => mount.mountPoint === volume.path)"
        :scan-disabled="scanActive || rebuildVolumeLoading || rebuildIndexLoading"
        @settings="edit(volume.id)"
        @scan="rebuildVolume(volume.id)"
      />
    </div>
    <div v-if="loadError" class="load-error">
      <p role="alert">{{ loadError }}</p>
      <v-text-button @click="reload">{{ $t('retry') }}</v-text-button>
    </div>
    <span v-else-if="!volumes.length" class="empty">{{ $t(loading ? 'loading' : 'no_data') }}</span>
  </section>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { initQuery } from '@/lib/api/query'
import { fileIndexVolumesGQL, canScanVolume, type FileIndexVolume, type FileIndexDefaults } from '@/lib/api/file-index'
import { createRefreshScheduler } from '@/lib/refresh-scheduler'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import type { IStorageMount } from '@/lib/interfaces'
import { getStorageVolumeTitle } from '@/lib/volumes'
import { pushModal } from '@/components/modal'
import { useScanAction } from '../home'
import FileIndexScopeDialog from './FileIndexScopeDialog.vue'
import FileIndexVolumeCard from './FileIndexVolumeCard.vue'

const props = defineProps<{ mounts: IStorageMount[] }>()
const { t } = useI18n()
const volumes = ref<FileIndexVolume[]>([])
const loadError = ref('')
const defaults = ref<FileIndexDefaults>()
const hasIndexScope = computed(() => volumes.value.some(canScanVolume))
const { loading, refetch: fetch } = initQuery({
  document: fileIndexVolumesGQL,
  variables: null,
  handle: (data: { fileIndexVolumes: FileIndexVolume[]; fileIndexDefaults: FileIndexDefaults }, error: string) => {
    loadError.value = error
    if (!error && data) {
      volumes.value = data.fileIndexVolumes
      defaults.value = data.fileIndexDefaults
    }
  },
})
const refresh = createRefreshScheduler(() => {
  void reload()
})
const stop = graphqlSubscriptions.subscribe('mediaUpdated', refresh.schedule)
onUnmounted(() => {
  stop()
  refresh.dispose()
})
const {
  scanProgress,
  scanActive,
  percent,
  stateLabel,
  showPause,
  showResume,
  showStop,
  showRebuild,
  rebuildIndexLoading,
  pauseScan,
  resumeScan,
  stopScan,
  rebuildIndex,
  rebuildVolume,
  rebuildVolumeLoading,
} = useScanAction()

async function reload() {
  await fetch(undefined, true)
}

function volumeName(volume: FileIndexVolume) {
  const mount = props.mounts.find((item) => item.mountPoint === volume.path)
  const name = mount ? getStorageVolumeTitle(mount, t) : volume.path
  return volume.isAvailable ? name : `${name} · ${t('offline')}`
}

function edit(id: string) {
  const volume = volumes.value.find((item) => item.id === id)
  if (!volume || !defaults.value) return
  pushModal(FileIndexScopeDialog, { volume, defaults: defaults.value, volumeName: volumeName(volume), onSaved: reload })
}
</script>

<style scoped>
.file-index-card {
  min-height: 120px;
  min-width: 0;
  padding: 16px;
}
header,
.actions,
.summary {
  display: flex;
  align-items: center;
}
header {
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 8px;
}
h2 {
  margin: 0;
  font-size: 0.875rem;
  font-weight: 500;
}
.actions {
  gap: 0;
}
.summary {
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 8px;
  margin-top: 8px;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.75rem;
}
.volume-cards {
  display: grid;
  gap: 8px;
  margin-top: 8px;
}
.load-error p {
  margin: 8px 0 0;
  overflow-wrap: anywhere;
  color: var(--md-sys-color-error);
}
.empty {
  color: var(--md-sys-color-on-surface-variant);
}
</style>
