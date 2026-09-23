<template>
  <SidebarListItem :title="channel.name" :subtitle="subtitle" :active="active" :force-actions="menuVisible" @click="emit('click')">
    <template #start>
      <span class="icon" aria-hidden="true">
        <v-dropdown v-model="infoOpen">
          <template #trigger>
            <i-lucide:hash />
          </template>
          <pre class="view-raw">{{ channel }}</pre>
        </v-dropdown>
      </span>
    </template>

    <template #end>
      <span v-if="time" v-tooltip="formatDateTime(time)" class="chat-time">{{ formatTimeAgo(time) }}</span>
    </template>
    <template #actions>
      <v-icon-button :id="anchorId" v-tooltip="$t('actions')" class="sm" @click.prevent.stop="showMenu">
        <i-material-symbols:more-vert />
      </v-icon-button>
      <v-dropdown-menu v-model="menuVisible" :anchor="anchorId">
        <template v-if="!confirmingDelete && !confirmingClear && !confirmingLeave">
          <div class="dropdown-item" @click="emitInfo">
            {{ $t('channel_info') }}
          </div>
          <div v-if="isOwner" class="dropdown-item" @click="openRename">
            {{ $t('rename') }}
          </div>
          <div class="dropdown-item" @click="confirmingClear = true">
            {{ $t('clear_messages') }}
          </div>
          <div v-if="isOwner || [ChannelStatus.KICKED, ChannelStatus.LEFT].includes(channel.status)" class="dropdown-item" @click="confirmingDelete = true">
            {{ $t('delete_channel') }}
          </div>
          <div v-else class="dropdown-item" @click="confirmingLeave = true">
            {{ $t('leave_channel') }}
          </div>
        </template>
        <template v-else-if="confirmingClear">
          <inline-delete-confirm
            :name="channel.name" :loading="clearLoading"
            message-key="clear_messages_confirm" @confirm="doClear" @cancel="confirmingClear = false" />
        </template>
        <template v-else-if="confirmingLeave">
          <inline-delete-confirm :name="channel.name" :loading="leaveLoading" message-key="leave_channel_confirm" @confirm="doLeave" @cancel="confirmingLeave = false" />
        </template>
        <template v-else>
          <inline-delete-confirm :name="channel.name" :loading="deleteLoading" message-key="delete_channel_confirm" @confirm="doDelete" @cancel="confirmingDelete = false" />
        </template>
      </v-dropdown-menu>
    </template>
  </SidebarListItem>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { useChatStore } from '@/stores/chat'
import { useTempStore } from '@/stores/temp'
import { replacePath } from '@/plugins/router'
import { deleteChatChannelGQL, leaveChatChannelGQL, initMutation } from '@/lib/api/mutation'
import { openModal } from '@/components/modal'
import { formatDateTime, formatTimeAgo } from '@/lib/format'
import { clearChatMessages, useTasks } from '../hooks/chat'
import RenameChannelModal from '@/views/chat/RenameChannelModal.vue'
import type { IChatChannel } from '@/lib/interfaces'
import { decryptChatId } from '../hooks/chat-route'
import { ChannelStatus } from '@/lib/status'

const props = defineProps<{
  channel: IChatChannel
  subtitle?: string
  active?: boolean
  time?: string
}>()

const emit = defineEmits<{
  (e: 'click'): void
  (e: 'info'): void
}>()

const router = useRouter()
const mainStore = useMainStore()
const chatStore = useChatStore()
const { urlTokenKey, app } = storeToRefs(useTempStore())

const currentChatId = computed(() => {
  if (router.currentRoute.value.path.includes('app-files')) return ''
  const qid = router.currentRoute.value.query.id
  const enc = typeof qid === 'string' && qid !== '' ? qid : ''
  return decryptChatId(enc, urlTokenKey.value)
})

const anchorId = computed(() => `channel-list-${props.channel.id}`)

const infoOpen = ref(false)
const menuVisible = ref(false)
const confirmingDelete = ref(false)
const deleteLoading = ref(false)
const confirmingClear = ref(false)
const clearLoading = ref(false)
const confirmingLeave = ref(false)
const leaveLoading = ref(false)
const { cancelByChatId } = useTasks()
const isOwner = computed(() => !!app.value?.clientId && props.channel.ownerId === app.value.clientId)

watch(menuVisible, (v) => {
  if (!v) {
    confirmingDelete.value = false
    confirmingClear.value = false
    confirmingLeave.value = false
  }
})

function showMenu() {
  const anchorElement = document.getElementById(anchorId.value)
  document.dispatchEvent(new CustomEvent('dropdown-toggle', { detail: { exclude: anchorElement } }))
  menuVisible.value = true
}

const { mutate: deleteChannel } = initMutation({ document: deleteChatChannelGQL })
const { mutate: leaveChannel } = initMutation({ document: leaveChatChannelGQL })

function doDelete() {
  if (deleteLoading.value) return
  deleteLoading.value = true
  const wasActive = currentChatId.value === `channel:${props.channel.id}`
  deleteChannel({ id: props.channel.id }).then((result) => {
    deleteLoading.value = false
    if (!result) return
    chatStore.removeChannel(props.channel.id)
    chatStore.fetchChannels()
    chatStore.fetchLatestChatItems()
    menuVisible.value = false
    if (wasActive) replacePath(mainStore, '/chat')
  })
}

function openRename() {
  menuVisible.value = false
  openModal(RenameChannelModal, {
    channel: props.channel,
    onRenamed: (renamed: IChatChannel) => chatStore.updateChannel(renamed),
  })
}

function emitInfo() {
  menuVisible.value = false
  emit('info')
}

async function doClear() {
  if (clearLoading.value) return
  clearLoading.value = true
  await clearChatMessages(`channel:${props.channel.id}`, cancelByChatId)
  clearLoading.value = false
  menuVisible.value = false
  chatStore.fetchLatestChatItems()
}

function doLeave() {
  if (leaveLoading.value) return
  leaveLoading.value = true
  const wasActive = currentChatId.value === `channel:${props.channel.id}`
  leaveChannel({ id: props.channel.id }).then((result) => {
    leaveLoading.value = false
    if (!result) return
    chatStore.removeChannel(props.channel.id)
    chatStore.fetchChannels()
    chatStore.fetchLatestChatItems()
    menuVisible.value = false
    if (wasActive) replacePath(mainStore, '/chat')
  })
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
