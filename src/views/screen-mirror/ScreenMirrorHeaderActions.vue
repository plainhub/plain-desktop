<template>
  <div class="actions">
    <template v-if="mirroring">
      <!-- Media Controls -->
      <div v-if="!showLoading" class="action-group">
        <v-icon-button v-tooltip="paused ? $t('play') : $t('pause')" @click="$emit('togglePlay')">
          <i-material-symbols:play-arrow-rounded v-if="paused" />
          <i-material-symbols:pause-rounded v-else />
        </v-icon-button>
        <v-dropdown v-if="!audioAvailable" v-model="audioMenuVisible">
          <template #trigger>
            <button class="btn-icon" disabled>
              <i-material-symbols:volume-off-rounded />
            </button>
          </template>
          <div class="warning-dropdown">
            <div class="warning-content">
              <i-material-symbols:error-outline-rounded />
              <div class="warning-text">{{ $t('mirror_audio_not_supported') }}</div>
            </div>
          </div>
        </v-dropdown>
        <v-icon-button v-else v-tooltip="muted ? $t('unmute') : $t('mute')" @click="$emit('toggleMute')">
          <i-material-symbols:volume-off-rounded v-if="muted" />
          <i-material-symbols:volume-up-rounded v-else />
        </v-icon-button>
        <v-icon-button v-tooltip="isFullscreen ? $t('exit_fullscreen') : $t('fullscreen')" @click="$emit('toggleFullscreen')">
          <i-material-symbols:fullscreen-exit-rounded v-if="isFullscreen" />
          <i-material-symbols:fullscreen-rounded v-else />
        </v-icon-button>
      </div>

      <!-- Navigation (when remote control enabled) -->
      <div v-if="controlEnabled && !showLoading" class="action-group">
        <v-icon-button v-tooltip="$t('nav_back')" @click="$emit('nav', 'BACK')"><i-material-symbols:arrow-back-rounded /></v-icon-button>
        <v-icon-button v-tooltip="$t('nav_home')" @click="$emit('nav', 'HOME')"><i-material-symbols:circle-outline /></v-icon-button>
        <v-icon-button v-tooltip="$t('nav_recents')" @click="$emit('nav', 'RECENTS')"><i-material-symbols:crop-square-outline /></v-icon-button>
      </div>

      <!-- Recording indicator -->
      <button v-if="recording" class="btn-recording" @click="$emit('toggleRecording')">
        <span class="recording-dot" />{{ recordingTime }}
      </button>

      <!-- More Menu -->
      <v-dropdown v-model="moreMenuVisible">
        <template #trigger>
          <v-icon-button><i-material-symbols:more-vert /></v-icon-button>
        </template>
        <div class="dropdown-item" :class="{ active: qualityMode === 'HD' }" @click="setQuality('HD')">
          {{ $t('mirror_hd') }}<i-material-symbols:check-rounded v-if="qualityMode === 'HD'" />
        </div>
        <div class="dropdown-item" :class="{ active: qualityMode === 'SMOOTH' }" @click="setQuality('SMOOTH')">
          {{ $t('mirror_smooth') }}<i-material-symbols:check-rounded v-if="qualityMode === 'SMOOTH'" />
        </div>
        <div class="dropdown-divider" />
        <div class="dropdown-item" @click="$emit('takeScreenshot'); moreMenuVisible = false">
          <i-material-symbols:screenshot-rounded />{{ $t('screenshot') }}
        </div>
        <div class="dropdown-item" @click="$emit('toggleRecording'); moreMenuVisible = false">
          <i-material-symbols:fiber-manual-record v-if="!recording" /><i-material-symbols:stop-rounded v-else />
          {{ recording ? $t('stop_recording') : $t('start_recording') }}
        </div>
        <template v-if="channel !== AppChannelType.GOOGLE">
          <div class="dropdown-divider" />
          <div class="dropdown-item" :class="{ active: controlEnabled }" @click="$emit('toggleControl'); moreMenuVisible = false">
            <i-material-symbols:touch-app-rounded />{{ $t('remote_control') }}<i-material-symbols:check-rounded v-if="controlEnabled" />
          </div>
        </template>
        <div class="dropdown-divider" />
        <div class="dropdown-item" @click="openKeyboardShortcuts">
          <i-lucide:keyboard />{{ $t('keyboard_shortcuts') }}
        </div>
      </v-dropdown>

      <!-- Stop Button -->
      <v-icon-button v-tooltip="$t('stop_mirror')" :disabled="stopServiceLoading" class="btn-stop" @click="$emit('stopService')">
        <i-material-symbols:stop-rounded />
      </v-icon-button>
    </template>
    <v-outlined-button v-else-if="!relaunchAppLoading && !idle" class="btn-sm" @click="$emit('relaunchApp')">{{ $t('relaunch_app') }}</v-outlined-button>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { openModal } from '@/components/modal'
import KeyboardShortcutsModal from '@/components/KeyboardShortcutsModal.vue'
import { AppChannelType } from '@/lib/status'
import { FEATURE } from '@/lib/data'
import { hasFeature } from '@/lib/feature'

const props = defineProps<{
  mirroring: boolean
  idle: boolean
  showLoading: boolean
  stopServiceLoading: boolean
  qualityMode: 'HD' | 'SMOOTH'
  recording: boolean
  recordingTime: string
  controlEnabled: boolean
  relaunchAppLoading: boolean
  channel: AppChannelType
  paused: boolean
  isFullscreen: boolean
  muted: boolean
  audioSupported: boolean
  osVersion: number
}>()

const audioAvailable = computed(() => props.audioSupported && hasFeature(FEATURE.MIRROR_AUDIO, props.osVersion))
const audioMenuVisible = ref(false)

const emit = defineEmits<{
  (e: 'stopService'): void
  (e: 'setQualityMode', mode: 'HD' | 'SMOOTH'): void
  (e: 'takeScreenshot'): void
  (e: 'toggleRecording'): void
  (e: 'toggleControl'): void
  (e: 'relaunchApp'): void
  (e: 'togglePlay'): void
  (e: 'toggleFullscreen'): void
  (e: 'toggleMute'): void
  (e: 'nav', action: 'BACK' | 'HOME' | 'RECENTS'): void
}>()

const moreMenuVisible = ref(false)

function setQuality(mode: 'HD' | 'SMOOTH') {
  emit('setQualityMode', mode)
  moreMenuVisible.value = false
}

function openKeyboardShortcuts() {
  moreMenuVisible.value = false
  openModal(KeyboardShortcutsModal, { shortcuts: mirrorShortcuts })
}

const mirrorShortcuts = [
  { keys: ['Click'], description: 'mirror_tap' },
  { keys: ['Click', '+', 'Drag'], description: 'mirror_swipe' },
  { keys: ['Scroll'], description: 'mirror_scroll' },
  { keys: ['Long press'], description: 'mirror_long_press' },
]
</script>

<style scoped lang="scss">
.actions { display: flex; gap: 8px; align-items: center; }
.action-group {
  display: flex; align-items: center; gap: 4px; padding-inline-end: 8px;
  border-inline-end: 1px solid color-mix(in srgb, var(--md-sys-color-on-surface) 12%, transparent);
}
.btn-icon:disabled {
  pointer-events: none;
  opacity: 0.38;
}
.btn-recording {
  display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px;
  border: 1px solid var(--md-sys-color-error); border-radius: 16px;
  background: transparent; color: var(--md-sys-color-error); font-size: 0.8rem; cursor: pointer;
  &:hover { background: color-mix(in srgb, var(--md-sys-color-error) 8%, transparent); }
}
.recording-dot {
  display: inline-block; width: 8px; height: 8px; border-radius: 50%;
  background: var(--md-sys-color-error); animation: blink 1s infinite;
}
@keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0.3; } }
</style>
