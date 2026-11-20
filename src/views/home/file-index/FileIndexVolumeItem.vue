<template>
  <FileIndexVolumeCard
    :volume="volume"
    :title="title"
    :mount="mount"
    :scan-progress="fileIndexScanProgress"
    :scan-disabled="loading || saving"
    @settings="$emit('settings')"
    @create="create"
    @scan="rebuild()"
    @pause="pause()"
    @resume="resume()"
    @stop="stop()"
  />
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { saveFileIndexVolume, type FileIndexVolume } from '@/lib/api/file-index'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import type { IStorageMount } from '@/lib/interfaces'
import { useScanAction } from '../home'
import FileIndexVolumeCard from './FileIndexVolumeCard.vue'

const { t } = useI18n()
const props = defineProps<{ volume: FileIndexVolume; title: string; mount?: IStorageMount }>()
const emit = defineEmits<{ settings: []; settled: [] }>()
const { fileIndexScanProgress, rebuild, loading, pause, resume, stop } = useScanAction(props.volume.id)
watch(fileIndexScanProgress, ({ state }) => {
  if (['IDLE', 'STOPPED', 'FAILED'].includes(state)) emit('settled')
})
const saving = ref(false)
async function create() {
  if (saving.value) return
  saving.value = true
  try {
    await saveFileIndexVolume({ ...props.volume, indexMode: 'BLOCKLIST' })
    emit('settled')
  } catch (error) {
    toast(t(error instanceof Error ? error.message : String(error)), 'error')
  } finally {
    saving.value = false
  }
}
</script>

