<template>
  <SidebarListItem :title="peer.name" :subtitle="subtitle" :active="active" :force-actions="menuVisible" @click="emit('click')">
    <template #start>
      <v-dropdown v-model="infoOpen">
        <template #trigger><PeerIcon :device-type="peer.deviceType" :online="online" :full-control="isManagedPeer" /></template>
        <pre class="view-raw">{{ peer }}</pre>
      </v-dropdown>
    </template>

    <template #end>
      <span v-if="time" v-tooltip="formatDateTime(time)" class="chat-time">{{ formatTimeAgo(time) }}</span>
    </template>
    <template #actions>
      <v-icon-button :id="anchorId" v-tooltip="$t('actions')" class="sm" @click.prevent.stop="showMenu">
        <i-material-symbols:more-horiz />
      </v-icon-button>
      <delete-confirm-popup
        v-model="menuVisible" :anchor="anchorId" :confirming="!!confirmAction"
        :name="peer.name" :loading="confirmAction === 'clear' ? clearLoading : deleteLoading" :message-key="confirmAction === 'clear' ? 'clear_messages_confirm' : 'delete_peer_warning'"
        @confirm="confirmAction === 'clear' ? doClear() : doDelete()" @cancel="confirmAction = ''"
      >
        <template #menu>
          <div class="dropdown-item" @click="emitInfo">
            {{ $t('device_info') }}
          </div>
          <div class="dropdown-item" @click="confirmAction = 'clear'">
            {{ $t('clear_messages') }}
          </div>
          <div class="dropdown-item" @click="confirmAction = 'delete'">
            {{ $t('delete_device') }}
          </div>
        </template>
      </delete-confirm-popup>
    </template>
  </SidebarListItem>
</template>

<script setup lang="ts">
import { showActionError } from '@/hooks/show-action-error'
import type { IPeer } from '@/lib/interfaces'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { useChatStore } from '@/stores/chat'
import { useTempStore } from '@/stores/temp'
import { findLoginPeer } from '@/lib/device/login-peers'
import { replacePath } from '@/plugins/router'
import { deletePeerGQL, initMutation } from '@/lib/api/mutation'
import { clearChatMessages, useTasks } from '../hooks/chat'
import { formatDateTime, formatTimeAgo } from '@/lib/format'
import { decryptChatId } from '../hooks/chat-route'

const props = defineProps<{
  peer: Pick<IPeer, 'id' | 'name' | 'deviceType' | 'managed'>
  subtitle?: string
  active?: boolean
  online?: boolean
  time?: string
}>()

const emit = defineEmits<{
  (e: 'click'): void
  (e: 'info'): void
}>()

const router = useRouter()
const mainStore = useMainStore()
const chatStore = useChatStore()
const { urlTokenKey } = storeToRefs(useTempStore())

const currentChatId = computed(() => {
  if (router.currentRoute.value.path.includes('app-files')) return ''
  const qid = router.currentRoute.value.query.id
  const enc = typeof qid === 'string' && qid !== '' ? qid : ''
  return decryptChatId(enc, urlTokenKey.value)
})

const anchorId = computed(() => `peer-list-${props.peer.id}`)

const isManagedPeer = computed(() => props.peer.managed === true || !!findLoginPeer(props.peer.id))

const infoOpen = ref(false)
const menuVisible = ref(false)
const confirmAction = ref<'clear' | 'delete' | ''>('')
const deleteLoading = ref(false)
const clearLoading = ref(false)
const { cancelByChatId } = useTasks()

watch(menuVisible, (v) => {
  if (!v) confirmAction.value = ''
})

function showMenu() {
  const anchorElement = document.getElementById(anchorId.value)
  document.dispatchEvent(new CustomEvent('dropdown-toggle', { detail: { exclude: anchorElement } }))
  menuVisible.value = true
}

const { mutate: deletePeer } = initMutation({ document: deletePeerGQL }, showActionError)

function doDelete() {
  if (deleteLoading.value) return
  deleteLoading.value = true
  const wasActive = currentChatId.value === `peer:${props.peer.id}`
  deletePeer({ peerId: props.peer.id }).then((result) => {
    deleteLoading.value = false
    if (!result) return
    chatStore.fetchPeers()
    chatStore.fetchLatestChatItems()
    menuVisible.value = false
    if (wasActive) replacePath(mainStore, '/chat')
  })
}

function emitInfo() {
  menuVisible.value = false
  emit('info')
}

async function doClear() {
  if (clearLoading.value) return
  clearLoading.value = true
  await clearChatMessages(`peer:${props.peer.id}`, cancelByChatId)
  clearLoading.value = false
  menuVisible.value = false
  chatStore.fetchLatestChatItems()
}
</script>
