<template>
  <SidebarListItem :title="title" :subtitle="subtitle" :active="active" :force-actions="menuVisible" @click="emit('click')">
    <template #start>
      <span class="icon" aria-hidden="true">
        <i-lucide:bot />
      </span>
    </template>

    <template #end>
      <span v-if="time" v-tooltip="formatDateTime(time)" class="chat-time">{{ formatTimeAgo(time) }}</span>
    </template>
    <template #actions>
      <v-icon-button :id="anchorId" v-tooltip="$t('actions')" class="sm" @click.prevent.stop="showMenu">
        <i-material-symbols:more-horiz />
      </v-icon-button>
      <delete-confirm-popup
        v-model="menuVisible" :anchor="anchorId" :confirming="confirmingClear"
        :name="title" :loading="clearLoading" :message-key="'clear_messages_confirm'"
        @confirm="doClear" @cancel="confirmingClear = false"
      >
        <template #menu>
          <div class="dropdown-item" @click="confirmingClear = true">
            {{ $t('clear_messages') }}
          </div>
        </template>
      </delete-confirm-popup>
    </template>
  </SidebarListItem>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import { useChatStore } from '@/stores/chat'
import { clearChatMessages, useTasks } from '../hooks/chat'
import { formatDateTime, formatTimeAgo } from '@/lib/format'

const props = defineProps<{
  title: string
  subtitle?: string
  active?: boolean
  time?: string
}>()

const emit = defineEmits<{
  (e: 'click'): void
}>()

const chatStore = useChatStore()
const { cancelByChatId } = useTasks()

const anchorId = 'chat-sidebar-local-actions'
const menuVisible = ref(false)
const confirmingClear = ref(false)
const clearLoading = ref(false)

watch(menuVisible, (v) => {
  if (!v) confirmingClear.value = false
})

function showMenu() {
  const anchorElement = document.getElementById(anchorId)
  document.dispatchEvent(new CustomEvent('dropdown-toggle', { detail: { exclude: anchorElement } }))
  menuVisible.value = true
}

async function doClear() {
  if (clearLoading.value) return
  clearLoading.value = true
  await clearChatMessages('peer:local', cancelByChatId)
  clearLoading.value = false
  menuVisible.value = false
  chatStore.fetchLatestChatItems()
}
</script>

<style lang="scss" scoped>
.icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
}
</style>
