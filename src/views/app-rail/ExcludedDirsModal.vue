<template>
  <v-modal @close="close">
    <template #headline>{{ $t('file_index_scope') }}</template>
    <template #content>
      <div class="index-scope">
        <p>{{ $t('excluded_dirs_hint') }}</p>
        <FileIndexStatus />
        <IndexDirectoriesSection :title="$t('index_default_directories')" :paths="store.fileIndexDefaults.scanDirectories" />
        <IndexDirectoriesSection :title="$t('index_added_directories')" :paths="store.indexDirectories" editable :busy="busy" @add="pick(false)" @remove="save(false, store.indexDirectories.filter((path) => path !== $event))" />
        <IndexDirectoriesSection :title="$t('index_default_exclusions')" :paths="[...store.fileIndexDefaults.excludedDirectories, ...store.fileIndexDefaults.excludedNames]" />
        <IndexDirectoriesSection :title="$t('exclude_directories')" :paths="store.excludedDirs" editable :busy="busy" @add="pick(true)" @remove="save(true, store.excludedDirs.filter((path) => path !== $event))" />
      </div>
    </template>
    <template #actions><v-outlined-button :disabled="busy" @click="close">{{ $t('close') }}</v-outlined-button></template>
  </v-modal>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { popModal, promptModal } from '@/components/modal'
import { useMainStore } from '@/stores/main'
import * as prefs from '@/lib/prefs/core'
import toast from '@/components/toaster'
import DirectoryPickerModal from '@/components/DirectoryPickerModal.vue'
import FileIndexStatus from '@/views/files/FileIndexStatus.vue'
import IndexDirectoriesSection from './IndexDirectoriesSection.vue'

const store = useMainStore()
const busy = ref(false)

async function save(excluded: boolean, paths: string[]) {
  busy.value = true
  try {
    await prefs.set(excluded ? 'file_index_excluded_dirs' : 'media_source_dirs', paths)
    if (excluded) store.excludedDirs = paths
    else store.indexDirectories = paths
  } catch (error) {
    toast(error instanceof Error ? error.message : String(error), 'error')
  } finally {
    busy.value = false
  }
}

async function pick(excluded: boolean) {
  const selected = await promptModal<string>(DirectoryPickerModal, {
    title: '', description: '', initialPath: '', modalId: 'index-directory-picker',
  })
  if (typeof selected !== 'string' || !selected.trim()) return
  const paths = excluded ? store.excludedDirs : store.indexDirectories
  if (!paths.includes(selected)) await save(excluded, [...paths, selected])
}

function close() { popModal() }
</script>

<style scoped>
.index-scope { display: flex; flex-direction: column; gap: 24px; min-width: 320px; }
p { margin: 0; }
</style>
