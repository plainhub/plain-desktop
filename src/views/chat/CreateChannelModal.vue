<template>
  <v-modal @close="close">
    <template #headline>{{ $t('create_channel') }}</template>
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
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { storeToRefs } from 'pinia'
import { popModal } from '@/components/modal'
import { initMutation, createChatChannelGQL } from '@/lib/api/mutation'
import { useChatStore } from '@/stores/chat'
import { useTempStore } from '@/stores/temp'
import { encryptChatId } from './hooks/chat-route'
import type { IChatChannel } from '@/lib/interfaces'

const router = useRouter()
const { urlTokenKey } = storeToRefs(useTempStore())
const name = ref('')
const chatStore = useChatStore()

const { mutate, loading, onDone } = initMutation({
  document: createChatChannelGQL,
})

onDone((r: any) => {
  const channel = { ...r.data.createChatChannel } as IChatChannel
  if (!chatStore.channels.some((c) => c.id === channel.id)) {
    chatStore.updateChannel(channel)
  }
  const routeId = encryptChatId(`channel:${channel.id}`, urlTokenKey.value)
  router.push(`/chat?id=${encodeURIComponent(routeId)}`)
  popModal()
})

function save() {
  if (!name.value.trim()) return
  mutate({ name: name.value.trim() })
}

function onEnterKey(e: KeyboardEvent) {
  if (e.isComposing) return
  save()
}

function close() {
  popModal()
}
</script>
