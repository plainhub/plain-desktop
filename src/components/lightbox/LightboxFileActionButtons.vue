<template>
  <div class="file-action-buttons">
    <template v-if="!readOnly && current && !current.isFromChat && !inZip">
      <template v-if="canDelete && (!canTrash || isTrashed)">
        <v-outlined-button :id="deleteAnchor" :disabled="deleting" @click.stop="requestDelete">
          <i-material-symbols:delete-forever-outline-rounded />
          {{ $t('delete') }}
        </v-outlined-button>
        <delete-confirm-popup v-model="confirming" :anchor="deleteAnchor" :name="current.name || current.data?.title || current.path" :loading="deleting" @confirm="onConfirmDelete" />
      </template>
      <v-outlined-button v-if="canTrash && isTrashed" :class="{ loading: restoreLoading(`ids:${current.data?.id}`) }" @click.stop="restoreItem">
        <i-material-symbols:restore-from-trash-outline-rounded />
        {{ $t('restore') }}
      </v-outlined-button>
      <template v-else>
        <v-outlined-button v-if="canTrash" :class="{ loading: trashLoading(`ids:${current.data?.id}`) }" @click.stop="trashMediaItem">
          <i-material-symbols:delete-outline-rounded />
          {{ $t('move_to_trash') }}
        </v-outlined-button>
        <v-outlined-button v-if="canMove" @click.stop="$emit('rename-file')">
          <i-material-symbols:edit-outline-rounded />
          {{ $t('rename') }}
        </v-outlined-button>
      </template>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, useId, watch } from 'vue'
import { isZipPath } from '@/lib/file'
import { DataType } from '@/lib/data'
import { useMediaRestore, useMediaTrash, useFileTrashState } from '@/hooks/media-trash'
import { useOrganizeUndo } from '@/hooks/organize-undo'
import emitter from '@/plugins/eventbus'
import type { IMediaItemsActionedEvent } from '@/lib/interfaces'
import type { ISource } from './types'

const props = defineProps<{
  current: ISource | undefined
  deleteFile: () => Promise<boolean>
  readOnly?: boolean
}>()

const emit = defineEmits(['rename-file', 'action-success'])

const deleteAnchor = useId()
const confirming = ref(false)
const deleting = ref(false)
watch(() => props.current?.path, () => { confirming.value = false }, { flush: 'sync' })
const inZip = computed(() => isZipPath(props.current?.path ?? ''))

function requestDelete() {
  if (props.readOnly || !props.current || props.current.isFromChat || inZip.value || deleting.value) return
  confirming.value = true
}
defineExpose({ requestDelete })

async function onConfirmDelete() {
  if (deleting.value) return
  deleting.value = true
  try {
    if (await props.deleteFile()) confirming.value = false
  } finally {
    deleting.value = false
  }
}

const { isTrashed, canTrash, canDelete, canMove } = useFileTrashState(() => props.current)

const { trash, trashLoading } = useMediaTrash()
const { record } = useOrganizeUndo()
async function trashMediaItem() {
  if (!props.current?.data?.id || !props.current.type) return
  const type = props.current.type as DataType
  const query = `ids:${props.current.data.id}`
  if (await trash(type, query)) record({ kind: 'trash', type, query })
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
</style> 