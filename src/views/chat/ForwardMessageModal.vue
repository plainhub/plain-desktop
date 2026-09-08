<template>
  <v-modal @close="close">
    <template #headline>{{ $t('forward_message') }}</template>
    <template #content>
      <div class="forward-list">
        <template v-if="channelTargets.length">
          <div class="forward-section">{{ $t('channels') }}</div>
          <div v-for="c in channelTargets" :key="c.id" class="forward-item" @click="toggle(`channel:${c.id}`)">
            <span class="name">{{ c.name }}</span>
            <v-checkbox :checked="selected.includes(`channel:${c.id}`)" @click.stop="toggle(`channel:${c.id}`)" />
          </div>
        </template>
        <template v-if="peerTargets.length">
          <div class="forward-section">{{ $t('devices') }}</div>
          <div v-for="p in peerTargets" :key="p.id" class="forward-item" @click="toggle(`peer:${p.id}`)">
            <span class="name">{{ p.name }}</span>
            <v-checkbox :checked="selected.includes(`peer:${p.id}`)" @click.stop="toggle(`peer:${p.id}`)" />
          </div>
        </template>
      </div>
    </template>
    <template #actions>
      <v-outlined-button @click="close">{{ $t('cancel') }}</v-outlined-button>
      <v-filled-button :disabled="selected.length === 0" @click="doForward">{{ $t('send') }}</v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { PropType } from 'vue'
import { popModal } from '@/components/modal'
import { initMutation, sendChatItemGQL } from '@/lib/api/mutation'
import { useChatStore } from '@/stores/chat'
import { messageNeedsDownload } from './hooks/chat-upload'
import { fetchLatestChatContent } from './hooks/forward-content'
import type { IChatItem } from '@/lib/interfaces'

const props = defineProps({
  message: { type: Object as PropType<IChatItem>, required: true },
  excludeChatId: { type: String, default: '' },
})

const chatStore = useChatStore()
const selected = ref<string[]>([])

const channelTargets = computed(() => chatStore.joinedChannels.filter((c) => `channel:${c.id}` !== props.excludeChatId))
const peerTargets = computed(() => chatStore.pairedPeers.filter((p) => `peer:${p.id}` !== props.excludeChatId))

const { mutate } = initMutation({
  document: sendChatItemGQL,
})

function toggle(id: string) {
  selected.value = selected.value.includes(id) ? selected.value.filter((s) => s !== id) : [...selected.value, id]
}

let submitting = false

async function doForward() {
  if (!selected.value.length || submitting) return
  submitting = true
  popModal()
  let content = props.message.content
  if (messageNeedsDownload(props.message)) {
    content = (await fetchLatestChatContent(props.message.id, props.excludeChatId)) ?? content
  }
  for (const toId of selected.value) {
    mutate({ toId, content })
  }
}

function close() {
  popModal()
}
</script>

<style lang="scss" scoped>
.forward-list {
  max-height: 320px;
  overflow-y: auto;
}

.forward-section {
  padding: 8px 4px 4px;
  font-size: 12px;
  font-weight: 500;
  color: var(--md-sys-color-outline);
}

.forward-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 4px 4px 4px 12px;
  border-radius: 8px;
  cursor: pointer;

  &:hover {
    background: var(--md-sys-color-surface-container-high);
  }

  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
}
</style>
