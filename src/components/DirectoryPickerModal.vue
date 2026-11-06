<template>
  <v-modal width="min(800px, calc(100vw - 32px))" @close="cancel">
    <template #headline>
      {{ title || $t('select_folder') }}
    </template>

    <template #content>
      <div class="picker">
        <p class="hint">{{ description || $t('folder_picker_hint') }}</p>
        <DirectoryBrowser
          :volumes="volumes"
          :loading-mounts="loadingMounts"
          :active-root="rootPath"
          :current-dir="currentDir"
          :can-go-up="canGoUp"
          :listing="listing"
          :listing-error="listingError"
          :dir-items="dirItems"
          :dir-name="dirName"
          @select-root="selectRoot"
          @go-up="goUp"
          @enter-dir="enterDir"
          @reload="reload"
        />
        <div v-if="recentDirs.length" class="recents">
          <div class="recents-label">{{ $t('recent_folders') }}</div>
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
import { useDirectoryPicker } from '@/hooks/directory-picker'

const props = defineProps({
  title: { type: String, default: '' },
  description: { type: String, default: '' },
  initialPath: { type: String, default: '' },
  recentDirs: { type: Array as () => string[], default: () => [] },
})

const emit = defineEmits<{
  (e: typeof Modal.EVENT_PROMPT, path: string): void
}>()

const { volumes, loadingMounts, rootPath, currentDir, canGoUp, listing, listingError, dirItems, selectRoot, selectDirectory, enterDir, goUp, dirName, reload } = useDirectoryPicker(props.initialPath)

function chooseCurrent() { emit(Modal.EVENT_PROMPT, currentDir.value) }
function cancel() { popModal() }
</script>

<style scoped>
.picker {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.hint {
  margin: 0;
  color: var(--md-sys-color-on-surface-variant);
}

.recents {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.recents-label {
  font-size: 12px;
  color: var(--md-sys-color-on-surface-variant);
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
  font-size: 13px;
}
</style>
