<template>
  <SidebarListItem :title="peer.name" :subtitle="subtitle" :active="active" :force-actions="menuVisible" @click="emit('click')">
    <template #start>
      <v-dropdown v-model="infoOpen">
        <template #trigger><PeerIcon :device-type="peer.deviceType" :online="online" :full-control="isLoginPeer" /></template>
        <pre class="view-raw">{{ peer }}</pre>
      </v-dropdown>
    </template>

    <template #end>
      <span v-if="time" v-tooltip="formatDateTime(time)" class="chat-time">{{ formatTimeAgo(time) }}</span>
    </template>
    <template #actions>
      <v-icon-button :id="anchorId" v-tooltip="$t('actions')" class="sm" @click.prevent.stop="showMenu">
        <i-material-symbols:more-vert />
      </v-icon-button>
      <v-dropdown-menu v-model="menuVisible" :anchor="anchorId">
        <template v-if="!confirmingDelete && !confirmingClear">
          <div class="dropdown-item" @click="emitInfo">
            {{ $t('device_info') }}
          </div>
          <div class="dropdown-item" @click="confirmingClear = true">
            {{ $t('clear_messages') }}
          </div>
          <div class="dropdown-item" @click="confirmingDelete = true">
            {{ $t('delete_device') }}
          </div>
        </template>
        <template v-else-if="confirmingClear">
          <inline-delete-confirm
            :name="peer.name" :loading="clearLoading"
            message-key="clear_messages_confirm" @confirm="doClear" @cancel="confirmingClear = false" />
        </template>
        <template v-else>
          <inline-delete-confirm :name="peer.name" :loading="deleteLoading" message-key="delete_peer_warning" @confirm="doDelete" @cancel="confirmingDelete = false" />
        </template>
      </v-dropdown-menu>
    </template>
  </SidebarListItem>
</template>

<script setup lang="ts">
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
  peer: Pick<IPeer, 'id' | 'name' | 'deviceType'>
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

const isLoginPeer = computed(() => !!findLoginPeer(props.peer.id))

const infoOpen = ref(false)
const menuVisible = ref(false)
const confirmingDelete = ref(false)
const deleteLoading = ref(false)
const confirmingClear = ref(false)
const clearLoading = ref(false)
const { cancelByChatId } = useTasks()

watch(menuVisible, (v) => {
  if (!v) {
    confirmingDelete.value = false
    confirmingClear.value = false
  }
})

function showMenu() {
  const anchorElement = document.getElementById(anchorId.value)
  document.dispatchEvent(new CustomEvent('dropdown-toggle', { detail: { exclude: anchorElement } }))
  menuVisible.value = true
}

const { mutate: deletePeer } = initMutation({ document: deletePeerGQL })

function doDelete() {
  if (deleteLoading.value) return
  deleteLoading.value = true
  const wasActive = currentChatId.value === `peer:${props.peer.id}`
  deletePeer({ id: props.peer.id }).then((result) => {
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
