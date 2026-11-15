<template>
  <section class="card file-index-card">
    <header>
      <h2>{{ $t('file_index.title') }}</h2>
    </header>
    <p class="index-hint">{{ $t('file_index.media_pages_hint') }}</p>
    <div v-if="mountedVolumes.length" class="volume-cards">
      <FileIndexVolumeItem
        v-for="{ mount, volume } in mountedVolumes"
        :key="volume.id"
        :volume="volume"
        :title="getStorageVolumeTitle(mount, t)"
        :mount="mount"
        @settings="edit(volume.id)"
        @settled="reload"
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
import { computed, onActivated, onDeactivated, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { initQuery } from '@/lib/api/query'
import { fileIndexVolumesGQL, type FileIndexVolume, type FileIndexDefaults } from '@/lib/api/file-index'
import type { IStorageMount } from '@/lib/interfaces'
import { getStorageVolumeTitle } from '@/lib/volumes'
import { pushModal } from '@/components/modal'
import FileIndexScopeDialog from './FileIndexScopeDialog.vue'
import FileIndexVolumeItem from './FileIndexVolumeItem.vue'
import { mountedIndexVolumes } from './mounted-volumes'

const props = defineProps<{ mounts: IStorageMount[] }>()
const { t } = useI18n()
const volumes = ref<FileIndexVolume[]>([])
const loadError = ref('')
const defaults = ref<FileIndexDefaults>()
const mountedVolumes = computed(() => mountedIndexVolumes(props.mounts, volumes.value))
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
let active = true
onDeactivated(() => { active = false })
onActivated(() => {
  if (active) return
  active = true
  void reload()
})
watch(() => JSON.stringify(props.mounts.map((mount) => [mount.id, mount.mountPoint])), reload)

async function reload() {
  if (!active) return
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
header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 8px;
}
h2 {
  margin: 0;
  font-size: 0.875rem;
  font-weight: 500;
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
.index-hint {
  margin: 8px 0 0;
  font-size: 0.75rem;
}
.index-hint,
.empty {
  color: var(--md-sys-color-on-surface-variant);
}
</style>
