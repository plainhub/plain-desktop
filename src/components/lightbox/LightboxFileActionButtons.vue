<template>
  <div class="file-action-buttons">
    <template v-if="!confirming">
      <template v-if="!readOnly && !current?.isFromChat && !inZip">
        <template v-if="canTrash">
          <template v-if="isTrashed">
            <v-outlined-button @click.stop="confirming = true">
              <i-material-symbols:delete-forever-outline-rounded />
              {{ $t('delete') }}
            </v-outlined-button>
            <v-outlined-button :class="{ loading: restoreLoading(`ids:${current?.data?.id}`) }" @click.stop="restoreItem">
              <i-material-symbols:restore-from-trash-outline-rounded />
              {{ $t('restore') }}
            </v-outlined-button>
          </template>
          <template v-else>
            <v-outlined-button :class="{ loading: trashLoading(`ids:${current?.data?.id}`) }" @click.stop="trashMediaItem">
              <i-material-symbols:delete-outline-rounded />
              {{ $t('move_to_trash') }}
            </v-outlined-button>
            <v-outlined-button @click.stop="$emit('rename-file')">
              <i-material-symbols:edit-outline-rounded />
              {{ $t('rename') }}
            </v-outlined-button>
          </template>
        </template>
        <template v-else>
          <v-outlined-button @click.stop="confirming = true">
            <i-material-symbols:delete-forever-outline-rounded />
            {{ $t('delete') }}
          </v-outlined-button>
          <v-outlined-button @click.stop="$emit('rename-file')">
            <i-material-symbols:edit-outline-rounded />
            {{ $t('rename') }}
          </v-outlined-button>
        </template>
      </template>
      <v-outlined-button class="download-btn" @click.stop="handleDownload">
        <i-material-symbols:download-rounded />
        {{ $t('download') }}
      </v-outlined-button>
    </template>
    <inline-delete-confirm v-else :name="current?.name ?? current?.data?.title ?? ''" @confirm="onConfirmDelete" @cancel="confirming = false" />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { getFileName } from '@/lib/api/file'
import { isZipPath } from '@/lib/file'
import { DataType } from '@/lib/data'
import { useMediaRestore, useMediaTrash, useFileTrashState } from '@/hooks/media-trash'
import { useOrganizeUndo } from '@/hooks/organize-undo'
import emitter from '@/plugins/eventbus'
import type { IMediaItemsActionedEvent } from '@/lib/interfaces'
import type { ISource } from './types'

const props = defineProps({
  current: {
    type: Object as () => ISource | undefined,
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

const emit = defineEmits(['rename-file', 'delete-file', 'action-success'])

const confirming = ref(false)
const inZip = computed(() => isZipPath(props.current?.path ?? ''))

function onConfirmDelete() {
  emit('delete-file')
  confirming.value = false
}

const { isTrashed, canTrash } = useFileTrashState(() => props.current, () => props.osVersion)

function handleDownload() {
  if (props.current?.path) {
    const fileName = (props.current.name ? props.current.name : getFileName(props.current.path)).replace(' ', '-')
    props.downloadFile(props.current.path, fileName)
  }
}

const { trash, trashLoading } = useMediaTrash()
const { record } = useOrganizeUndo()
function trashMediaItem() {
  if (!props.current?.data?.id || !props.current.type) return
  trash(props.current.type as DataType, `ids:${props.current.data.id}`)
  record({ kind: 'trash', type: props.current.type as DataType, query: `ids:${props.current.data.id}` })
}

const { restore, restoreLoading } = useMediaRestore()
function restoreItem() {
  if (!props.current?.data?.id || !props.current.type) return
  restore(props.current.type as DataType, `ids:${props.current.data.id}`)
}

// Listen for media action events and emit action-success
const mediaItemsActionedHandler = (event: IMediaItemsActionedEvent) => {
  const currentQuery = `ids:${props.current?.data?.id}`
  if (event.query === currentQuery && (event.action === 'trash' || event.action === 'restore')) {
    emit('action-success', event.action)
  }
}

onMounted(() => {
  emitter.on('media_items_actioned', mediaItemsActionedHandler)
})

onUnmounted(() => {
  emitter.off('media_items_actioned', mediaItemsActionedHandler)
})
</script>

<style lang="scss" scoped>
.file-action-buttons {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  grid-template-rows: auto auto;
  gap: 8px;
}

.download-btn {
  grid-column: 1 / span 2;

  &:first-child {
    grid-column: 1 / span 2;
  }
}
</style> 