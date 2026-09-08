<template>
  <v-dropdown v-model="open" :max-height="500">
    <template #trigger>
      <v-icon-button :aria-label="$t('settings')">
        <i-lucide:settings-2  />
      </v-icon-button>
    </template>

    <div class="top-app-bar">
      <div class="title">
        <span>{{ app.deviceName || currentSession?.name || $t('my_phone') }}</span>
        <button
          v-tooltip="$t('edit')"
          class="switch-device-btn"
          :aria-label="$t('edit')"
          @click="editDeviceName"
        >
          <i-lucide:pencil />
        </button>
        <button
          v-if="isTauri"
          v-tooltip="$t('device_discovery.change_device')"
          class="switch-device-btn"
          :aria-label="$t('device_discovery.change_device')"
          @click="openDeviceSwitcher"
        >
          <i-lucide:arrow-left-right />
        </button>
      </div>
      <div v-if="!localMode && app?.battery != null" class="actions">
        <svg class="popup-battery-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <rect x="1" y="6.5" width="18" height="11" rx="2" ry="2" />
          <line x1="23" y1="10" x2="23" y2="14" />
          <rect x="2.5" y="8" :width="14 * (app.battery / 100)" height="8" rx="1" fill="currentColor" stroke="none" />
        </svg>
        <span class="popup-battery-pct">{{ app.battery }}%</span>
      </div>
    </div>

    <div class="theme-row">
      <theme-changer />
    </div>

    <div
      class="dropdown-item language-trigger"
      :class="{ open: languageOpen }"
      @mouseenter="openLanguage"
      @mouseleave="scheduleCloseLanguage"
    >
      <i-lucide:globe class="feature-icon" />
      <span>{{ $t('header_actions.language') }}</span>
      <i-lucide:chevron-right class="feature-icon-end" />
      <teleport to="body">
        <div
          v-if="languageOpen"
          class="language-menu"
          :style="languageMenuStyle"
          @mouseenter="cancelCloseLanguage"
          @mouseleave="scheduleCloseLanguage"
        >
          <div
            v-for="lang in availableLocales"
            :key="lang.code"
            class="dropdown-item language-item"
            :class="{ selected: lang.code === locale }"
            @click="onLangClick(lang.code)"
          >
            <span>{{ lang.name }}</span>
          </div>
        </div>
      </teleport>
    </div>

    <template v-if="!localMode">
      <div v-if="!isTauri" class="dropdown-item" @click="logout">
        <i-material-symbols:logout-rounded class="feature-icon" />
        <span>{{ $t('header_actions.logout') }}</span>
      </div>

      <div class="popup-divider"></div>

      <div class="dropdown-item" @click="openCustomizeUI">
        <i-lucide:layout-list class="feature-icon" />
        <span>{{ $t('customize_ui') }}</span>
      </div>

      <div class="dropdown-item" @click="openExcludedDirs">
        <i-lucide:folder-minus class="feature-icon" />
        <span>{{ $t('exclude_directories') }}</span>
      </div>
    </template>

    <router-link to="/developer" class="dropdown-item" @click="open = false">
      <i-lucide:code-2 class="feature-icon" />
      <span>{{ $t('developer.developer_mode') }}</span>
    </router-link>

    <template v-if="!localMode && popupFeatures.length">
      <div class="popup-divider"></div>
      <router-link
        v-for="feat in popupFeatures"
        :key="feat.id"
        :to="lastRoute(feat.defaultPath, feat.group)"
        class="dropdown-item"
        @click="open = false"
      >
        <component :is="feat.icon" class="feature-icon" />
        <span>{{ $t(feat.titleKey) }}</span>
      </router-link>
    </template>

    <div v-if="showAbout" class="dropdown-item" @click="openAbout">
      <i-lucide:info class="feature-icon" />
      <span>{{ $t('menu.about') }}</span>
    </div>
  </v-dropdown>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useTempStore } from '@/stores/temp'
import { useMainStore } from '@/stores/main'
import { findLoginPeer, updateLoginPeerName } from '@/lib/device/login-peers'
import { getRemoteClientId } from '@/lib/device/client-id'
import { storeToRefs } from 'pinia'
import { pushModal, openModal } from '@/components/modal'
import { AppChannelType } from '@/lib/status'
import { getAvailableFeatures, type Feature } from './features'
import { isLocalMode } from '@/lib/device/local-mode'
import { isMacPlatform } from '@/lib/platform'
import { openAboutWindow } from '@/lib/api/tauri-window'
import { clearCurrentSession } from '@/lib/device/current'
import { clear as prefsClear } from '@/lib/prefs'
import { useLocaleSwitch } from '@/composables/useLocaleSwitch'
import emitter from '@/plugins/eventbus'
import ThemeChanger from '@/components/ThemeChanger.vue'
import CustomizeUIModal from './CustomizeUIModal.vue'
import ExcludedDirsModal from './ExcludedDirsModal.vue'
import DeviceSwitcherModal from '@/components/DeviceSwitcherModal.vue'
import EditValueModal from '@/components/EditValueModal.vue'
import { initMutation, updateDeviceNameGQL } from '@/lib/api/mutation'

const localMode = isLocalMode()
const { t } = useI18n()

const { app } = storeToRefs(useTempStore())
const store = useMainStore()
const currentSession = computed(() => findLoginPeer(getRemoteClientId()))
const router = useRouter()
const open = ref(false)
const isTauri = __IS_TAURI__
const showAbout = isTauri && !isMacPlatform()

const { locale, availableLocales, handleLocaleSwitch } = useLocaleSwitch()

const languageOpen = ref(false)
const languageTriggerRef = ref<HTMLElement | null>(null)
const languageMenuStyle = ref<Record<string, string>>({ position: 'fixed', visibility: 'hidden' })
let closeTimer: ReturnType<typeof setTimeout> | null = null

function positionLanguageMenu() {
  const trigger = languageTriggerRef.value
  if (!trigger) return
  const rect = trigger.getBoundingClientRect()
  const MENU_WIDTH = 240
  const GAP = 4
  const ITEM_HEIGHT = 40
  const MENU_PADDING = 8
  const MENU_MAX_HEIGHT = 320
  const VIEWPORT_MARGIN = 8
  const vh = window.innerHeight
  const estimatedMenuHeight = Math.min(
    MENU_MAX_HEIGHT,
    availableLocales.value.length * ITEM_HEIGHT + MENU_PADDING,
  )
  const left = Math.min(window.innerWidth - MENU_WIDTH - VIEWPORT_MARGIN, rect.right + GAP)
  let top
  if (rect.top + estimatedMenuHeight + VIEWPORT_MARGIN <= vh) {
    top = rect.top
  } else {
    top = Math.max(VIEWPORT_MARGIN, rect.bottom - estimatedMenuHeight)
  }
  languageMenuStyle.value = {
    position: 'fixed',
    top: `${top}px`,
    left: `${Math.max(VIEWPORT_MARGIN, left)}px`,
    width: `${MENU_WIDTH}px`,
    visibility: 'visible',
    zIndex: '10000',
  }
}

function openLanguage(e: MouseEvent) {
  if (closeTimer) {
    clearTimeout(closeTimer)
    closeTimer = null
  }
  languageTriggerRef.value = e.currentTarget as HTMLElement
  languageOpen.value = true
  void nextTick(() => positionLanguageMenu())
}

function scheduleCloseLanguage() {
  if (closeTimer) clearTimeout(closeTimer)
  closeTimer = setTimeout(() => {
    languageOpen.value = false
    closeTimer = null
  }, 120)
}

function cancelCloseLanguage() {
  if (closeTimer) {
    clearTimeout(closeTimer)
    closeTimer = null
  }
}

function onLangClick(code: string) {
  languageOpen.value = false
  open.value = false
  void handleLocaleSwitch(code)
}

function logout() {
  open.value = false
  clearCurrentSession()
  prefsClear()
  window.location.reload()
}

onUnmounted(() => {
  if (closeTimer) clearTimeout(closeTimer)
})

const popupFeatures = computed<Feature[]>(() => {
  const available = getAvailableFeatures(app.value?.channel ?? AppChannelType.GITHUB, app.value?.debug ?? false)
  return available.filter((f) => !store.railFeatures.includes(f.id))
})

function openCustomizeUI() {
  open.value = false
  pushModal(CustomizeUIModal)
}

function openExcludedDirs() {
  open.value = false
  pushModal(ExcludedDirsModal)
}

function openDeviceSwitcher() {
  open.value = false
  pushModal(DeviceSwitcherModal)
}

function openAbout() {
  open.value = false
  void openAboutWindow()
}

function editDeviceName() {
  open.value = false
  openModal(EditValueModal, {
    title: t('device_name'),
    placeholder: t('device_name'),
    value: app.value?.deviceName ?? currentSession.value?.name ?? '',
    mutation: () =>
      initMutation({
        document: updateDeviceNameGQL,
      }),
    getVariables: (value: string) => ({ name: value }),
    done: (value: string) => {
      if (app.value) app.value.deviceName = value
      void updateLoginPeerName(getRemoteClientId(), value)
      emitter.emit('device_name_updated', value)
    },
  })
}

function lastRoute(defaultPath: string, group: string) {
  try {
    const saved = store.lastRoutes[group]
    if (!saved) return defaultPath
    const resolved = router.resolve(saved)
    const g = (resolved.meta?.group || '') as string
    return g === group ? saved : defaultPath
  } catch {
    return defaultPath
  }
}
</script>

<style lang="scss" scoped>
.top-app-bar {
  width: 300px;
}

.top-app-bar .title {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.switch-device-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  cursor: pointer;

  svg {
    width: 16px;
    height: 16px;
  }

  &:hover {
    background: color-mix(in srgb, var(--md-sys-color-primary) 12%, transparent);
    color: var(--md-sys-color-primary);
  }
}

.popup-battery-svg {
  width: 20px;
  height: 20px;
  flex-shrink: 0;
  color: var(--md-sys-color-primary);
}

.popup-battery-pct {
  font-size: 0.8125rem;
}

.popup-divider {
  height: 1px;
  background: var(--md-sys-color-outline-variant);
  margin: 4px 0;
}

.theme-row {
  padding: 4px 0 8px;

  :deep(.theme-changer) {
    margin: 0 16px;
  }
}

.language-trigger {
  cursor: default;

  .feature-icon-end {
    margin-left: auto;
    width: 18px;
    height: 18px;
    color: var(--md-sys-color-on-surface-variant);
  }

  &.open {
    background-color: var(--md-sys-color-surface-variant);
  }
}

</style>

<style lang="scss">
.language-menu {
  background: var(--md-sys-color-surface-container);
  border-radius: 8px;
  box-shadow: 0 4px 24px rgba(0, 0, 0, 0.18);
  padding: 4px 0;
  max-height: 320px;
  overflow-y: auto;

  .dropdown-item.language-item {
    padding: 8px 16px;
  }
}
</style>
