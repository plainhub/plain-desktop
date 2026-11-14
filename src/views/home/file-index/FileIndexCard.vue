<template>
  <section class="card file-index-card">
    <header>
      <h2>{{ $t('file_index.title') }}</h2>
      <div class="actions">
        <v-icon-button v-if="showPause" v-tooltip="$t('pause')" :aria-label="$t('pause')" @click="pauseScan"><i-material-symbols:pause-rounded /></v-icon-button>
        <v-icon-button v-if="showResume" v-tooltip="$t('resume')" :aria-label="$t('resume')" @click="resumeScan"><i-material-symbols:play-arrow-rounded /></v-icon-button>
        <v-icon-button v-if="showStop" v-tooltip="$t('stop')" :aria-label="$t('stop')" @click="stopScan"><i-material-symbols:stop-rounded /></v-icon-button>
        <v-text-button v-if="mountedVolumes.length > 1 && (rebuildIndexLoading || (hasIndexScope && showRebuild))" :loading="rebuildIndexLoading" :disabled="rebuildVolumeLoading" @click="rebuildIndex">{{
          $t('rebuild_index')
        }}</v-text-button>
      </div>
    </header>
    <div v-if="stateLabel" class="summary" aria-live="polite">
      <span
        >{{ stateLabel }}<template v-if="scanActive && fileIndexScanProgress.total !== null && fileIndexScanProgress.total > 0"> · {{ percent }}%</template></span
      >
      <span v-if="fileIndexScanProgress.indexed > 0 || (fileIndexScanProgress.total !== null && fileIndexScanProgress.total > 0)"
        >{{ fileIndexScanProgress.indexed.toLocaleString() }}<template v-if="fileIndexScanProgress.total !== null && fileIndexScanProgress.total > 0"> / {{ fileIndexScanProgress.total.toLocaleString() }}</template></span
      >
    </div>
    <div v-if="mountedVolumes.length" class="volume-cards">
      <FileIndexVolumeCard
        v-for="{ mount, volume } in mountedVolumes"
        :key="volume.id"
        :volume="volume"
        :title="getStorageVolumeTitle(mount, t)"
        :mount="mount"
        :scan-disabled="scanActive || rebuildVolumeLoading || rebuildIndexLoading"
        @settings="edit(volume.id)"
        @scan="rebuildVolume(volume.id)"
      />
    </div>
    <div v-if="loadError" class="load-error">
      <p role="alert">{{ loadError }}</p>
      <v-text-button @click="reload">{{ $t('retry') }}</v-text-button>
    </div>
    <span v-else-if="!mountedVolumes.length" class="empty">{{ $t(loading ? 'loading' : 'no_data') }}</span>
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
import { mountedIndexVolumes } from './mounted-volumes'

const props = defineProps<{ mounts: IStorageMount[] }>()
const { t } = useI18n()
const volumes = ref<FileIndexVolume[]>([])
const loadError = ref('')
const defaults = ref<FileIndexDefaults>()
const mountedVolumes = computed(() => mountedIndexVolumes(props.mounts, volumes.value))
const hasIndexScope = computed(() => mountedVolumes.value.some(({ volume }) => canScanVolume(volume)))
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
  fileIndexScanProgress,
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

function edit(id: string) {
  const entry = mountedVolumes.value.find(({ volume }) => volume.id === id)
  if (!entry || !defaults.value) return
  pushModal(FileIndexScopeDialog, { volume: entry.volume, defaults: defaults.value, volumeName: getStorageVolumeTitle(entry.mount, t), onSaved: reload })
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
