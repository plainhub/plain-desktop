<template>
  <v-icon-button v-if="single" v-tooltip="$t('download')" :loading="loading" @click.stop="$emit('download')">
    <i-material-symbols:download-rounded />
  </v-icon-button>
  <v-dropdown v-else v-model="menuVisible" strategy="below">
    <template #trigger>
      <v-icon-button v-tooltip="$t('download')" :loading="loading">
        <i-material-symbols:download-rounded />
      </v-icon-button>
    </template>
    <div class="context-menu">
      <div class="dropdown-item" @click="choose('download-each')">
        <i-material-symbols:download-rounded class="dropdown-item-icon" />
        {{ $t('download_individually') }}
      </div>
      <div class="dropdown-item" @click="choose('download-zip')">
        <i-material-symbols:folder-zip-outline-rounded class="dropdown-item-icon" />
        {{ $t('download_as_zip') }}
      </div>
      <div v-if="isTauri" class="dropdown-item divided" @click="changeDownloadDir">
        <i-material-symbols:folder-open-outline-rounded class="dropdown-item-icon" />
        {{ $t('choose_download_dir') }}
      </div>
    </div>
  </v-dropdown>
</template>

<script setup lang="ts">
import { ref } from 'vue'

const isTauri = __IS_TAURI__

defineProps<{
  single: boolean
  loading?: boolean
}>()

const emit = defineEmits<{
  download: []
  'download-each': []
  'download-zip': []
}>()

const menuVisible = ref(false)

function choose(event: 'download-each' | 'download-zip') {
  menuVisible.value = false
  if (event === 'download-each') emit('download-each')
  else emit('download-zip')
}

async function changeDownloadDir() {
  menuVisible.value = false
  const { chooseDownloadDir } = await import('@/lib/download-dir')
  await chooseDownloadDir()
}
</script>
