<template>
  <div class="q-action">
    <v-dropdown v-model="menuVisible">
      <template #trigger>
        <v-icon-button id="menu-ref" v-tooltip="t('settings')">
          <i-lucide:ellipsis-vertical />
        </v-icon-button>
      </template>

      <!-- Quick Actions Section -->
      <div v-if="isTablet && props.loggedIn" class="dropdown-section">
        <div class="dropdown-section-title">{{ t('quick_actions') }}</div>
        <div v-if="hasTasks || store.quick === 'upload'" class="dropdown-item" :class="{ selected: store.quick === 'upload' }" @click="toggleQuick('upload')">
          <i-material-symbols:format-list-numbered-rounded />
          {{ t('header_actions.uploads') }}
        </div>
        <div class="dropdown-item" :class="{ selected: store.quick === 'audio' }" @click="toggleQuick('audio')">
          <i-material-symbols:queue-music-rounded />
          {{ t('playlist') }}
        </div>
        <div class="dropdown-item" :class="{ selected: store.quick === 'pomodoro' }" @click="toggleQuick('pomodoro')">
          <i-material-symbols:timer-outline />
          {{ t('pomodoro_timer') }}
        </div>
        <div class="dropdown-item" :class="{ selected: store.quick === 'bookmark' }" @click="toggleQuick('bookmark')">
          <i-lucide:bookmark />
          {{ t('bookmarks') }}
        </div>
        <div class="dropdown-item" :selected="{ active: store.quick === 'chat' }" @click="toggleQuick('chat')">
          <i-lucide:bot />
          {{ t('my_phone') }}
        </div>
      </div>

      <!-- Logout Section -->
      <div v-if="props.loggedIn && !isTauri" class="dropdown-section">
        <div class="dropdown-item" @click="logout">
          <i-material-symbols:logout-rounded />
          {{ t('header_actions.logout') }}
        </div>
      </div>

      <!-- Theme Section -->
      <div class="dropdown-section">
        <div class="dropdown-section-title">{{ t('header_actions.theme') }}</div>
        <theme-changer />
      </div>

      <!-- Language Section -->
      <div class="dropdown-section">
        <div class="dropdown-section-title">{{ t('header_actions.language') }}</div>
        <div
          v-for="lang in availableLocales"
          :key="lang.code"
          class="dropdown-item"
          :class="{ selected: lang.code === locale }"
          @click="onLangClick(lang.code)"
        >
          {{ lang.name }}
        </div>
      </div>
    </v-dropdown>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, inject } from 'vue'
import { useI18n } from 'vue-i18n'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { clear as prefsClear } from '@/lib/prefs'
import { useLocaleSwitch } from '@/composables/useLocaleSwitch'

const props = defineProps({
  loggedIn: { type: Boolean },
})

const emit = defineEmits(['toggle-quick'])

const store = useMainStore()
const tempStore = useTempStore()
const { app } = storeToRefs(tempStore)
const menuVisible = ref(false)
const { t } = useI18n()

const isTablet = inject('isTablet')
const isTauri = __IS_TAURI__
const hasTasks = computed(() => {
  return tempStore.uploads.length > 0
})

const { locale, availableLocales, handleLocaleSwitch } = useLocaleSwitch()

function toggleQuick(name: string) {
  menuVisible.value = false
  emit('toggle-quick', name)
}

function onLangClick(code: string) {
  menuVisible.value = false
  void handleLocaleSwitch(code)
}

function logout() {
  menuVisible.value = false
  prefsClear()
  window.location.reload()
}
</script>

<style lang="scss" scoped>
.dropdown-section {
  &:not(:last-child) {
    border-bottom: 1px solid var(--border-color);
  }
}

.dropdown-section-title {
  font-size: 0.75rem;
  font-weight: 500;
  color: var(--md-sys-color-on-surface-variant);
  padding: 8px 12px 4px;
  text-transform: uppercase;
}
</style>
