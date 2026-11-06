<template>
  <v-modal width="min(800px, calc(100vw - 32px))" @close="cancel">
    <template #headline>
      {{ title || $t('select_folder') }}
    </template>

    <template #content>
      <div class="picker">
        <p class="subtle">{{ description || $t('folder_picker_hint') }}</p>
        <DirectoryBrowser
          :volumes="volumes"
          :volume="volume"
          :loading-mounts="loadingMounts"
          :active-root="rootPath"
          :current-dir="currentDir"
          :listing="listing"
          :listing-error="listingError"
          :dir-items="dirItems"
          :dir-name="dirName"
          @select-root="selectRoot"
          @enter-dir="enterDir"
          @reload="reload"
        />
        <div v-if="recentDirs.length" class="recents">
          <div class="subtle">{{ $t('recent_folders') }}</div>
          <v-text-button v-for="d in recentDirs" :key="d" class="recent-item" :disabled="loadingMounts" @click="selectDirectory(d)">
            <template #icon><i-material-symbols:folder-outline-rounded /></template>
            <span class="recent-path">{{ d }}</span>
          </v-text-button>
        </div>
      </div>
    </template>

    <template #actions>
      <v-text-button value="cancel" @click="cancel">{{ $t('cancel') }}</v-text-button>
      <v-filled-button :disabled="loadingMounts || listing || !!listingError || !rootPath" value="ok" @click="chooseCurrent">
        {{ $t('select_this_folder') }}
      </v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { Modal, popModal } from '@/components/modal'
import DirectoryBrowser from '@/components/DirectoryBrowser.vue'
import { useDirectoryPicker, type DirectoryPickerVolume } from '@/hooks/directory-picker'

const props = withDefaults(defineProps<{
  title?: string
  description?: string
  initialPath?: string
  recentDirs?: string[]
  volume?: DirectoryPickerVolume
}>(), {
  title: '',
  description: '',
  initialPath: '',
  recentDirs: () => [],
  volume: undefined,
})

const emit = defineEmits<{
  (e: typeof Modal.EVENT_PROMPT, path: string): void
}>()

const { volumes, loadingMounts, rootPath, currentDir, listing, listingError, dirItems, selectRoot, selectDirectory, enterDir, dirName, reload } = useDirectoryPicker(props.initialPath, props.volume)

function chooseCurrent() { emit(Modal.EVENT_PROMPT, currentDir.value) }
function cancel() { popModal() }
</script>

<style scoped>
.picker {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.recents {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.recent-item {
  justify-content: flex-start;
  gap: 8px;
  height: auto;
  min-height: 32px;
  text-align: start;
}

.recent-path {
  overflow-wrap: anywhere;
}
</style>
