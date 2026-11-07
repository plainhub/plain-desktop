<template>
  <div class="tauri-tab-bar">
    <div class="left-controls">
      <button class="bar-btn" title="Back" @click="goBack">
        <i-material-symbols:arrow-back-ios-new-rounded />
      </button>
      <button class="bar-btn" title="Forward" @click="goForward">
        <i-material-symbols:arrow-forward-ios-rounded />
      </button>
      <button class="bar-btn" title="Refresh" @click="refreshPage">
        <i-material-symbols:refresh-rounded />
      </button>
    </div>

    <div class="tabs-row">
    <div
      v-for="tab in mainStore.tabs"
      :key="tab.id"
      class="tab-item"
      :class="{ active: tab.id === mainStore.activeTabId, home: tab.id === 'home' }"
      @click="activateTab(tab)"
      @contextmenu="onTabContextMenu($event, tab)"
    >
      <template v-if="tab.id === 'home'">
        <span class="tab-title">{{ homeTabTitle }}</span>
        <span class="tab-switch-divider" />
        <v-dropdown v-model="homeDeviceMenuOpen" strategy="below">
          <template #trigger>
            <button class="tab-switch" :title="$t('device_discovery.change_device')">
              <i-material-symbols:keyboard-arrow-down-rounded />
            </button>
          </template>
          <div class="dropdown-item" @click="switchToLocal">
            <span>{{ selfDevice?.name }}</span>
            <span class="status-badge on">{{ $t('device_discovery.local') }}</span>
          </div>
          <div
            v-for="session in switchableSessions"
            :key="session.id"
            class="dropdown-item"
            @click="switchToSession(session.id)"
          >
            {{ session.name || peerHost(session) }}
          </div>
          <div class="dropdown-item" @click="openNearbyDevices">
            {{ $t('manage_devices') }}
          </div>
        </v-dropdown>
      </template>
      <template v-else>
        <span class="tab-title">{{ tab.title }}</span>
      </template>
      <button v-if="tab.closeable" class="tab-close" @click.stop="closeTab(tab.id)">
        <i-material-symbols:close-small-rounded />
      </button>
    </div>
    </div>

    <div class="tab-spacer" data-tauri-drag-region />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useMainStore } from '@/stores/main'
import type { AppTab } from '@/stores/main'
import { loginPeers, findLoginPeer, peerHost } from '@/lib/device/login-peers'
import { sortByName } from '@/lib/array'
import { getRemoteClientId, setRemoteClientId, clearRemoteClientId } from '@/lib/device/client-id'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { pushModal } from '@/components/modal'
import NearbyView from '@/views/chat/NearbyView.vue'
import i18n from '@/plugins/i18n'
import { loadSelfDevice, type SelfDevice } from '@/lib/device/self-device'
import { contextmenu } from '@/components/contextmenu'

const router = useRouter()
const route = useRoute()
const mainStore = useMainStore()
const { app } = storeToRefs(useTempStore())
const homeDeviceMenuOpen = ref(false)
const selfDevice = ref<SelfDevice | null>(null)

const currentSession = computed(() => findLoginPeer(getRemoteClientId()))

const homeTabTitle = computed(() => {
  const session = currentSession.value
  return (
    app.value?.deviceName
    || session?.name
    || (session ? peerHost(session) : '')
  )
})

const switchableSessions = computed(() => sortByName(loginPeers.value, (p) => p.name))

onMounted(() => {
  window.addEventListener('tauri-open-tab', handleOpenTab)
  void loadSelf()
  // Sync current route on first mount (e.g. page reload)
  const group = (route.meta?.group as string) || ''
  if (group && group !== 'home') {
    const titleKey = `page_title.${group}`
    const resolved = String((i18n.global as any).t(titleKey))
    mainStore.syncRouteTab(group, resolved !== titleKey ? resolved : group, route.fullPath)
  }
})

onUnmounted(() => {
  window.removeEventListener('tauri-open-tab', handleOpenTab)
})

function activateTab(tab: AppTab) {
  mainStore.setActiveTab(tab.id)
  if (route.fullPath !== tab.path) {
    router.push(tab.path)
  }
}

function closeTab(id: string) {
  const navPath = mainStore.closeTab(id)
  if (navPath != null) {
    router.push(navPath)
  }
}

function closeTabsToRight(id: string) {
  const navPath = mainStore.closeTabsToRight(id)
  if (navPath != null) {
    router.push(navPath)
  }
}

function onTabContextMenu(e: MouseEvent, tab: AppTab) {
  e.preventDefault()
  const idx = mainStore.tabs.findIndex((t) => t.id === tab.id)
  if (idx === -1) return
  if (!mainStore.tabs.slice(idx + 1).some((t) => t.closeable)) return
  contextmenu({
    x: e.clientX,
    y: e.clientY,
    items: [
      {
        label: String(i18n.global.t('close_tabs_to_the_right')),
        onClick: () => closeTabsToRight(tab.id),
      },
    ],
  })
}

function switchToSession(clientId: string) {
  homeDeviceMenuOpen.value = false
  setRemoteClientId(clientId)
  // Full reload so all stores/sockets re-init against the selected device.
  window.location.href = '/'
}

function switchToLocal() {
  homeDeviceMenuOpen.value = false
  clearRemoteClientId()
  window.location.href = '/'
}

async function loadSelf() {
  selfDevice.value = await loadSelfDevice()
}

function openNearbyDevices() {
  homeDeviceMenuOpen.value = false
  pushModal(NearbyView)
}

function handleOpenTab(e: Event) {
  const path = (e as CustomEvent<{ path: string }>).detail.path
  const resolved = router.resolve(path)
  const group = (resolved.meta?.group as string) || ''
  if (group && group !== 'home') {
    const titleKey = `page_title.${group}`
    const title = String((i18n.global as any).t(titleKey))
    mainStore.syncRouteTab(group, title !== titleKey ? title : group, path)
  } else {
    mainStore.openFileTab(path, path)
  }
  router.push(path)
}

function goBack() {
  router.back()
}

function goForward() {
  router.forward()
}

function refreshPage() {
  router.go(0)
}
</script>

<style lang="scss" scoped>
.tauri-tab-bar {
  flex-shrink: 0;
  height: var(--pl-tauri-tab-bar-height, 38px);
  display: flex;
  align-items: center;
  gap: 2px;
  padding-left: 80px; /* leave room for macOS traffic lights */
  padding-right: 8px;
  background: var(--md-sys-color-surface-container);
  -webkit-app-region: drag;
}

.left-controls {
  display: flex;
  align-items: center;
  gap: 2px;
  -webkit-app-region: no-drag;
}

.tabs-row {
  display: flex;
  align-items: center;
  gap: 2px;
  min-width: 0;
}

.bar-btn {
  -webkit-app-region: no-drag;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 6px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  cursor: pointer;

  svg {
    width: 16px;
    height: 16px;
  }

  &:hover {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }
}

.tab-item {
  -webkit-app-region: no-drag;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 0 6px 0 12px;
  height: 28px;
  border-radius: 6px;
  cursor: pointer;
  max-width: 200px;
  min-width: 40px;
  color: var(--md-sys-color-on-surface-variant);
  transition: background 0.12s;

  &:hover {
    background: var(--md-sys-color-surface-container-high);
  }

  &.active {
    background: var(--md-sys-color-surface-container-highest);
    color: var(--md-sys-color-on-surface);
  }
}

.tab-item.home {
  color: var(--md-sys-color-on-surface);
  box-shadow: inset 0 0 0 1px var(--md-sys-color-outline-variant);
  margin-inline-end: 4px;
}

.tab-item.home.active .tab-title {
  color: var(--md-sys-color-primary);
}

.tab-item.home .tab-title {
  font-weight: 600;
}

.tab-title {
  font-size: 12px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  user-select: none;
  -webkit-user-select: none;
}

.tab-close {
  -webkit-app-region: no-drag;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border: none;
  background: transparent;
  border-radius: 50%;
  cursor: pointer;
  color: inherit;
  padding: 0;
  opacity: 0;
  flex-shrink: 0;
  transition: opacity 0.12s, background 0.12s;

  .tab-item:hover & {
    opacity: 0.7;
  }

  &:hover {
    opacity: 1 !important;
    background: var(--md-sys-color-surface-variant);
  }
}

.tab-switch {
  -webkit-app-region: no-drag;
  width: 16px;
  height: 16px;
  border: none;
  background: transparent;
  border-radius: 50%;
  color: inherit;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  cursor: pointer;
  opacity: 0.75;
  transition: background 0.12s, opacity 0.12s;

  &:hover {
    opacity: 1;
    background: var(--md-sys-color-surface-variant);
    color: var(--md-sys-color-primary);
  }
}

.tab-switch-divider {
  width: 1px;
  height: 12px;
  background: var(--md-sys-color-outline-variant);
  flex-shrink: 0;
}

.dropdown-divider {
  height: 1px;
  margin: 4px 0;
  background: var(--md-sys-color-outline-variant);
}

:deep(.dropdown-item),
:deep(.dropdown-divider) {
  -webkit-app-region: no-drag;
}

.tab-spacer {
  flex: 1;
  height: 100%;
  -webkit-app-region: drag;
  min-width: 20px;
}
</style>
