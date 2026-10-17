<template>
  <div class="actions">
    <template v-if="!confirming">
      <v-icon-button v-tooltip="$t('delete')" @click.stop="confirming = true">
        <i-material-symbols:delete-forever-outline-rounded />
      </v-icon-button>
      <v-icon-button v-tooltip="$t('edit')" @click.stop="edit">
        <i-material-symbols:edit />
      </v-icon-button>
      <TagRelationsDropdown :type="dataType" :tags="tags" :item="{ key: item.id }" :selected="item.tags" />
    </template>
    <inline-delete-confirm v-else :name="getContactFullName(item)" @confirm="onConfirmDelete" @cancel="confirming = false" />
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import type { IContact, ITag } from '@/lib/interfaces'
import { DataType } from '@/lib/data'
import { getContactFullName } from '@/lib/contact/format'

interface Props {
  item: IContact
  tags: ITag[]
  dataType: DataType
}

const props = defineProps<Props>()

const emit = defineEmits<{
  deleteItem: [item: IContact]
  edit: [item: IContact]
}>()

const confirming = ref(false)

function onConfirmDelete() {
  emit('deleteItem', props.item)
  confirming.value = false
}

function edit() {
  emit('edit', props.item)
}
</script> 