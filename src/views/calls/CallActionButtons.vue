<template>
  <div class="actions">
    <v-icon-button :id="deleteAnchor" v-tooltip="$t('delete')" @click.stop="confirming = true">
      <i-material-symbols:delete-forever-outline-rounded />
    </v-icon-button>
    <v-icon-button v-tooltip="$t('make_a_phone_call')" :loading="callLoading && callId === item.id" @click.stop="call">
      <i-material-symbols:call-outline-rounded />
    </v-icon-button>
    <TagRelationsDropdown :type="dataType" :tags="tags" :item="{ key: item.id }" :selected="item.tags" />
    <delete-confirm-popup v-model="confirming" :anchor="deleteAnchor" :name="item.number" @confirm="onConfirmDelete" />
  </div>
</template>

<script setup lang="ts">
import { ref, useId } from 'vue'
import type { ICall, ITag } from '@/lib/interfaces'
import { DataType } from '@/lib/data'

interface Props {
  item: ICall
  tags: ITag[]
  dataType: DataType
  callLoading?: boolean
  callId?: string
}

const props = defineProps<Props>()

const emit = defineEmits<{
  deleteItem: [item: ICall]
  call: [item: ICall]
}>()

const deleteAnchor = useId()
const confirming = ref(false)

function onConfirmDelete() {
  emit('deleteItem', props.item)
  confirming.value = false
}

function call() {
  emit('call', props.item)
}
</script> 