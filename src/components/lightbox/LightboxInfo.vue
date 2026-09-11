<template>
  <section class="info">
    <div class="info-header">
      <div class="info-title">
        <span>{{ $t('info') }}</span>
        <lightbox-keyboard-shortcuts class="info-keyboard-shortcuts" />
      </div>
      <div class="info-actions">
        <LightboxFileActionButtons 
          :current="current" 
          :os-version="osVersion"
          :read-only="readOnly"
          :download-file="downloadFile"
          @rename-file="$emit('rename-file')"
          @delete-file="$emit('delete-file')"
        />
      </div>
    </div>
    <div class="info-content">
      <LightboxFileDetails 
        :current="current" 
        :file-info="fileInfo" 
        :app-dir="appDir" 
      />

      <LightboxMoveToFolders
        v-if="showMoveToFolders"
        :current="current"
        @moved="onMoved"
      />
      
      <LightboxFileTags 
        :current="current" 
        :file-info="fileInfo"
        :tags-map="tagsMap"
      />
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue'
import { isZipPath } from '@/lib/file'
import { DataType } from '@/lib/data'
import { useMediaTrash, useFileTrashState } from '@/hooks/media-trash'
import type { ITag } from '@/lib/interfaces'
import type { ISource } from './types'
import LightboxMoveToFolders from './LightboxMoveToFolders.vue'

const props = defineProps({
  current: {
    type: Object as () => ISource | undefined,
    required: true,
  },
  fileInfo: {
    type: Object,
    default: null,
  },
  urlTokenKey: {
    type: String,
    required: true,
  },
  appDir: {
    type: String,
    default: '',
  },
  tagsMap: {
    type: Object as () => Map<string, ITag[]>,
    required: true,
  },
  osVersion: {
    type: Number,
    default: 0,
  },
  readOnly: {
    type: Boolean,
    default: false,
  },
  downloadFile: {
    type: Function,
    required: true,
  },
})

const emit = defineEmits(['rename-file', 'delete-file', 'refetch-info'])

const { isTrashed, canTrash } = useFileTrashState(() => props.current, () => props.osVersion)
const { trash } = useMediaTrash()
const inZip = computed(() => isZipPath(props.current?.path ?? ''))
const showMoveToFolders = computed(
  () => !props.readOnly && !inZip.value && props.current?.type === DataType.IMAGE && !isTrashed.value,
)

function onMoved() {
  emit('refetch-info')
}

function handleKeyDown(event: KeyboardEvent) {
  if (props.readOnly || inZip.value) return
  if (event.key === 'Delete' || ((event.ctrlKey || event.metaKey) && event.key === 'Backspace')) {
    event.preventDefault()
    
    if (canTrash.value) {
      if (isTrashed.value) {
        // Already in trash, permanently delete
        emit('delete-file')
      } else {
        // Not in trash, move to trash
        if (props.current?.data?.id && props.current.type) {
          trash(props.current.type as DataType, `ids:${props.current.data.id}`)
        }
      }
    } else {
      // Can't trash, directly delete
      emit('delete-file')
    }
  }
}

onMounted(() => {
  document.addEventListener('keydown', handleKeyDown)
})

onUnmounted(() => {
  document.removeEventListener('keydown', handleKeyDown)
})
</script>

<style lang="scss" scoped>
.info {
  grid-area: info;
  width: 350px;
  height: 100vh;
  box-sizing: border-box;
  background: var(--md-sys-color-surface-container);
  z-index: 1;
  display: flex;
  flex-direction: column;
}

.info-header {
  padding: 8px 16px;
}

.info-title {
  margin: 0 0 16px 0;
  display: flex;
  align-items: center;
  gap: 16px;
  span {
    font-size: 1.2rem;
    font-weight: bold;
  }
}

.info-content {
  padding: 16px;
  flex: 1;
  overflow-y: auto;
}

.info-keyboard-shortcuts {
  margin-inline-start: auto;
}
</style>
