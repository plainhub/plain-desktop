<template>
  <FileIndexVolumeCard
    :volume="volume"
    :title="title"
    :mount="mount"
    :scan-progress="fileIndexScanProgress"
    :scan-disabled="loading"
    @settings="$emit('settings')"
    @scan="rebuild()"
    @pause="pause()"
    @resume="resume()"
    @stop="stop()"
  />
</template>

<script setup lang="ts">
import { watch } from 'vue'
import type { FileIndexVolume } from '@/lib/api/file-index'
import type { IStorageMount } from '@/lib/interfaces'
import { useScanAction } from '../home'
import FileIndexVolumeCard from './FileIndexVolumeCard.vue'

const props = defineProps<{ volume: FileIndexVolume; title: string; mount?: IStorageMount }>()
const emit = defineEmits<{ settings: []; settled: [] }>()
const { fileIndexScanProgress, rebuild, loading, pause, resume, stop } = useScanAction(props.volume.id)
watch(fileIndexScanProgress, ({ state }) => {
  if (['IDLE', 'STOPPED', 'FAILED'].includes(state)) emit('settled')
})
</script>
