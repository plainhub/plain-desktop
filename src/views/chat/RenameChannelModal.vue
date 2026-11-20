<template>
  <v-modal @close="close">
    <template #headline>{{ $t('rename_channel') }}</template>
    <template #content>
      <v-text-field v-model="name" :label="$t('channel_name')" autofocus @keydown.enter="onEnterKey" />
    </template>
    <template #actions>
      <v-outlined-button @click="close">{{ $t('cancel') }}</v-outlined-button>
      <v-filled-button :loading="loading" :disabled="!name.trim()" @click="save">{{ $t('save') }}</v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { showActionError } from '@/hooks/show-action-error'
import { ref } from 'vue'
import type { PropType } from 'vue'
import { popModal } from '@/components/modal'
import { initMutation, updateChatChannelGQL } from '@/lib/api/mutation'
import type { IChatChannel } from '@/lib/interfaces'

const props = defineProps({
  channel: { type: Object as PropType<IChatChannel>, required: true },
  onRenamed: { type: Function as PropType<(channel: IChatChannel) => void>, default: undefined },
})

const name = ref(props.channel.name)

const { mutate, loading, onDone } = initMutation({
  document: updateChatChannelGQL,
}, showActionError)

onDone((r: any) => {
  const renamed = { ...r.data.updateChatChannel }
  props.onRenamed?.(renamed)
  popModal()
})

function save() {
  if (!name.value.trim()) return
  mutate({ id: props.channel.id, name: name.value.trim() })
}

function onEnterKey(e: KeyboardEvent) {
  if (e.isComposing) return
  save()
}

function close() {
  popModal()
}
</script>
