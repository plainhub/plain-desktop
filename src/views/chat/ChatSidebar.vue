<template>
  <Teleport v-if="isActive" to="#header-start-slot" defer>
    <v-icon-button id="chat-sidebar-add-ref" v-tooltip="$t('create_channel')" @click="() => (addMenuVisible = true)">
      <i-material-symbols:add-rounded />
    </v-icon-button>
    <v-dropdown-menu v-model="addMenuVisible" anchor="chat-sidebar-add-ref">
      <div v-for="item in actionItems" :key="item.text" class="dropdown-item" @click="item.click(); addMenuVisible = false">
        {{ $t(item.text) }}
      </div>
    </v-dropdown-menu>
  </Teleport>
  <left-sidebar>
    <template #body>
      <ul class="nav">
        <SidebarLocalListItem
          :active="currentChatId === 'peer:local'" :title="$t('page_title.local_chat')"
          :subtitle="getLatestChatPreview('peer:local')" :time="getLatestChatCreatedAt('peer:local')"
          @click="openChat('peer:local')" />
      </ul>

      <template v-if="loading">
        <div class="sidebar-loading">
          <v-circular-progress indeterminate class="sm" />
        </div>
      </template>
      <template v-else>
        <template v-if="channels.length > 0">
          <div class="section-title">
            {{ $t('channels') }}
          </div>
          <ul class="nav">
            <SidebarChannelListItem
              v-for="channel in channels" :key="channel.id" :channel="channel"
              :active="isChannelActive(channel.id)" :subtitle="getLatestChatPreview(`channel:${channel.id}`)"
              :time="getLatestChatCreatedAt(`channel:${channel.id}`)"
              @click="openChat(getChannelChatRouteId(channel.id))"
              @info="openInfo(`channel:${channel.id}`)" />
          </ul>
        </template>
        <template v-if="allPeers.length > 0">
          <div class="section-title">
            {{ $t('devices') }}
          </div>
          <ul class="nav">
            <SidebarPeerListItem
              v-for="peer in allPeers" :key="peer.id" :peer="peer" :active="isPeerActive(peer.id)"
              :online="!!peer.online" :subtitle="getLatestChatPreview(`peer:${peer.id}`) || getPeerIp(peer)"
              :time="getLatestChatCreatedAt(`peer:${peer.id}`)" @click="openChat(getPeerChatRouteId(peer.id))"
              @info="openInfo(`peer:${peer.id}`)" />
          </ul>
        </template>
      </template>
    </template>
  </left-sidebar>
</template>

<script setup lang="ts">
import { computed, onActivated, onDeactivated, onMounted, ref } from 'vue'
import { storeToRefs } from 'pinia'
import { useRouter } from 'vue-router'
import { openModal } from '@/components/modal'
import { getPeerIp } from '@/lib/device/login-peers'
import { replacePath } from '@/plugins/router'
import { decryptChatId, encryptChatId } from './hooks/chat-route'
import { useChatStore } from '@/stores/chat'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import SidebarChannelListItem from './components/SidebarChannelListItem.vue'
import SidebarPeerListItem from './components/SidebarPeerListItem.vue'
import SidebarLocalListItem from './components/SidebarLocalListItem.vue'
import CreateChannelModal from './CreateChannelModal.vue'
import NearbyView from './NearbyView.vue'
import PeerInfoModal from './PeerInfoModal.vue'
import ChannelInfoModal from './ChannelInfoModal.vue'

const router = useRouter()
const mainStore = useMainStore()
const { urlTokenKey } = storeToRefs(useTempStore())
const chatStore = useChatStore()
const { loading, allPeers, channels, peers } = storeToRefs(chatStore)

const addMenuVisible = ref(false)
const isActive = ref(false)

const currentChatId = computed(() => {
  if (router.currentRoute.value.path.includes('app-files')) return ''
  const qid = router.currentRoute.value.query.id
  const enc = typeof qid === 'string' && qid !== '' ? qid : ''
  return decryptChatId(enc, urlTokenKey.value)
})

function isPeerActive(peerId: string) {
  return currentChatId.value === `peer:${peerId}`
}

function isChannelActive(channelId: string) {
  return currentChatId.value === `channel:${channelId}`
}

function getPeerChatRouteId(peerId: string) {
  return encryptChatId(`peer:${peerId}`, urlTokenKey.value)
}

function getChannelChatRouteId(channelId: string) {
  return encryptChatId(`channel:${channelId}`, urlTokenKey.value)
}

function openChat(id: string) {
  if (id === 'peer:local') {
    replacePath(mainStore, '/chat')
    return
  }
  replacePath(mainStore, `/chat?id=${encodeURIComponent(id)}`)
}

function openInfo(targetChatId: string) {
  if (targetChatId.startsWith('peer:')) {
    const peerId = targetChatId.slice(5)
    openModal(PeerInfoModal, {
      peer: peerId ? chatStore.findPeer(peerId) : null,
    })
  } else if (targetChatId.startsWith('channel:')) {
    const channelId = targetChatId.slice(8)
    const channel = chatStore.findChannel(channelId)
    if (!channel) return
    openModal(ChannelInfoModal, {
      channel,
      peers: peers.value,
      onMemberUpdated: () => chatStore.fetchChannels(),
    })
  }
}

function getLatestChatPreview(chatId: string) {
  return chatStore.getLatestChatPreview(chatId)
}

function getLatestChatCreatedAt(chatId: string) {
  return chatStore.getLatestChatCreatedAt(chatId)
}

function openCreateChannel() {
  openModal(CreateChannelModal)
}

async function openNearby() {
  openModal(NearbyView)
}

const actionItems = computed(() => [
  { text: 'create_channel', click: openCreateChannel },
  { text: 'pair_device', click: openNearby },
])

onMounted(() => { chatStore.init() })
onActivated(() => { isActive.value = true })
onDeactivated(() => { isActive.value = false })
</script>

<style lang="scss" scoped>
.sidebar-loading {
  display: flex;
  justify-content: center;
  padding: 16px;
}
</style>