<template>
  <v-modal @close="popModal">
    <template #content>
      {{ $t('confirm_to_delete_name', { name: truncateText(files.map((it) => it.name).join(', '), 200) }) }}
    </template>
    <template #actions>
      <v-outlined-button @click="popModal">{{ $t('cancel') }}</v-outlined-button>
      <v-filled-button :loading="loading" @click="doDelete">
        {{ $t('delete') }}
      </v-filled-button>
    </template>
  </v-modal>
</template>
<script setup lang="ts">
import type { PropType } from 'vue'
import type { IFile } from '@/lib/file'
import { initMutation, deleteFilesGQL } from '@/lib/api/mutation'
import { popModal } from './modal'
import { truncateText } from '@/lib/array'

const props = defineProps({
  onDone: {
    type: Function as PropType<(file: IFile[]) => void>,
    required: true,
  },
  onError: {
    type: Function as PropType<() => void>,
    required: true,
  },
  files: {
    type: Array as PropType<Array<IFile>>,
    required: true,
  },
})

const {
  mutate,
  loading,
  onDone: onDeleteFilesDone,
  onError: onDeleteFilesError,
} = initMutation({
  document: deleteFilesGQL,
})

function doDelete() {
  mutate({ paths: props.files.map((it) => it.path) })
}

onDeleteFilesError(() => props.onError())

onDeleteFilesDone(() => {
  props.onDone(props.files)
  popModal()
})
</script>
