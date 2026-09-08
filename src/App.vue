<template>
  <div v-if="wsStatus" class="top-error">
    {{ $t('fix_disconnect_tips') }}
  </div>
  <router-view />
  <Teleport to="body">
    <modal-container />
    <div v-if="tapPhoneMessage" v-click-away="closeTapPhone" class="tap-phone-container" @click="closeTapPhone">
      <div>
        {{ tapPhoneMessage }}
      </div>
      <TouchPhone />
    </div>
  </Teleport>
</template>
<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import { useAppSocket } from '@/hooks/app-socket'
import { openWindow, setWindowDeviceName } from '@/lib/api/tauri-window'
import { updateDocumentTitle } from '@/plugins/router'
import { loginPeers, peerHost } from '@/lib/device/login-peers'
import { getRemoteClientId } from '@/lib/device/client-id'
import { isLocalMode } from '@/lib/device/local-mode'
import { useChatStore } from '@/stores/chat'
import { startLocalPeerData } from '@/lib/peer/local-peer-data'
import { useDeviceDiscovery } from './hooks/use-device-discovery'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'

const { wsStatus, tapPhoneMessage, closeTapPhone } = useAppSocket()
// Initialize the device pairing listener (and global incoming-request modal) early
// so pairing requests are handled even if the user never opens the Nearby modal.
useDeviceDiscovery()
// Eagerly instantiate the chat store so the global eventbus listeners
// (`channels_updated`, `peer_status_updated`, `channel_invite_received`,
// message events) are registered for the whole app lifetime — not only
// after the user navigates to the chat page. Without this, invites
// arriving while the user is on another page would be dropped silently.
useChatStore()
// Resident local-mode peer data (notification aggregation today, clipboard
// later): per-peer fetch + WS for the whole app lifetime, independent of
// whether a panel is open.
if (isLocalMode()) startLocalPeerData()
const { app } = storeToRefs(useTempStore())

function onKeydown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === 'N') {
    e.preventDefault()
    openWindow('/')
  }
}

// Keep the dock right-click menu label up to date for this window.
// Device switches always go through a page reload, so reading the bound
// clientId once per login-peers change is enough.
watch(
  [loginPeers, () => app.value?.deviceName],
  ([peers]) => {
    const session = peers.find((s) => s.id === getRemoteClientId())
    const name = isLocalMode()
      ? (app.value?.deviceName || 'PlainApp')
      : (session?.name || (session ? peerHost(session) : '') || 'PlainApp')
    setWindowDeviceName(name)
    updateDocumentTitle()
  },
  { immediate: true },
)

onMounted(() => window.addEventListener('keydown', onKeydown))
onUnmounted(() => window.removeEventListener('keydown', onKeydown))
</script>

<style scoped>
.top-error {
  background-color: var(--md-sys-color-error);
  color: var(--md-sys-color-on-error);
  padding: 8px;
  font-size: 0.8rem;
  display: flex;
  align-items: center;
  justify-content: center;
}
</style>
