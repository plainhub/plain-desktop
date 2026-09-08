<template>
  <div class="screen-mirror">
    <Teleport v-if="isActive" to="#header-start-slot" defer>
      <ScreenMirrorHeaderStart
        :mirroring="mirroring"
        :audio-requesting="audioRequesting"
        :audio-supported="audioSupported"
        :os-version="app.osVersion"
        :permissions="app.permissions"
        @request-audio-permission="requestAudioPermission"
      />
    </Teleport>

    <Teleport v-if="isActive" to="#header-end-slot" defer>
      <ScreenMirrorHeaderActions
        :mirroring="mirroring"
        :idle="idle"
        :show-loading="showLoading"
        :stop-service-loading="stopServiceLoading"
        :quality-mode="qualityMode"
        :recording="recording"
        :recording-time="recordingTime"
        :control-enabled="controlEnabled"
        :relaunch-app-loading="relaunchAppLoading"
        :channel="app.channel"
        :paused="paused"
        :is-fullscreen="isFullscreen"
        :muted="muted"
        :audio-supported="audioSupported"
        :os-version="app.osVersion"
        @stop-service="stopService"
        @set-quality-mode="setQualityMode"
        @take-screenshot="takeScreenshot"
        @toggle-recording="toggleRecording"
        @toggle-control="toggleControl"
        @relaunch-app="relaunchApp"
        @toggle-play="togglePlay"
        @toggle-fullscreen="toggleFullscreen"
        @toggle-mute="toggleMute"
        @nav="sendNavAction"
      />
    </Teleport>

    <ScreenMirrorContent
      :show-loading="showLoading"
      :idle="idle"
      :seconds="seconds"
      :failed="failed"
      :mirroring="mirroring"
      :control-enabled="controlEnabled"
      :supported="supported"
      :need-https="needHttps"
      :set-canvas-ref="setCanvasRef"
      :set-audio-ref="setAudioRef"
      :set-control-overlay-ref="setControlOverlayRef"
      @start="start"
      @use-https="useHttpsLink"
    />
  </div>
</template>

<script setup lang="ts">
import ScreenMirrorContent from './ScreenMirrorContent.vue'
import ScreenMirrorHeaderActions from './ScreenMirrorHeaderActions.vue'
import ScreenMirrorHeaderStart from './ScreenMirrorHeaderStart.vue'
import emitter from '@/plugins/eventbus'
import { useTempStore } from '@/stores/temp'
import { buildUrl } from '@/lib/url'
import { storeToRefs } from 'pinia'
import type { ComponentPublicInstance } from 'vue'
import { computed, onActivated, onDeactivated, ref, watch } from 'vue'
import { useScreenMirrorControl, type ScreenMirrorControlAction } from './screen-mirror-control'
import { useScreenMirrorMedia } from './screen-mirror-media'
import { useScreenMirrorService } from './screen-mirror-service'
import { useScreenMirrorPipeline } from './screen-mirror-pipeline'
import { useScreenRecording } from './screen-recording'

const { app } = storeToRefs(useTempStore())
const isActive = ref(false)
const canvasRef = ref<HTMLCanvasElement>()
const audioRef = ref<HTMLAudioElement>()
const controlOverlayRef = ref<HTMLDivElement>()

const { recording, recordingTime, toggleRecording } = useScreenRecording(canvasRef)

const media = useScreenMirrorMedia(canvasRef, audioRef)
const muted = media.muted
const isFullscreen = media.isFullscreen
const takeScreenshot = media.takeScreenshot
const toggleFullscreen = media.toggleFullscreen

const service = useScreenMirrorService()
const seconds = service.seconds
const controlEnabled = service.controlEnabled
const audioRequesting = service.audioRequesting
const relaunchAppLoading = service.relaunchLoading
const stopServiceLoading = service.stopLoading
const requestAudioPermission = service.requestAudioPermission
const setQualityMode = service.setQualityMode
const toggleControl = service.toggleControl
const relaunchApp = service.relaunchApp
const start = service.start

const pipeline = useScreenMirrorPipeline(canvasRef, audioRef, service.onStreamReady, service.onDisconnected, service.onScreenMirrorOff)
const paused = pipeline.paused
const supported = pipeline.supported
const audioSupported = pipeline.audioSupported
const togglePlay = pipeline.togglePlay

const needHttps = computed(() => !supported.value && !window.isSecureContext)
const useHttpsLink = () => {
  window.open(buildUrl('https', window.location.hostname, app.value.httpsPort), '_blank')
}

service.setPipeline(pipeline.connect, pipeline.cleanup)

const control = useScreenMirrorControl(canvasRef, service.controlEnabled)

const showLoading = computed(() => service.showLoading.value || service.state.value === 'connecting')
const mirroring = computed(() => service.state.value === 'streaming')
const failed = computed(() => service.state.value === 'failed')
const idle = computed(() => service.state.value === 'idle')
const qualityMode = computed<'HD' | 'SMOOTH'>(() => service.qualityMode.value as 'HD' | 'SMOOTH')

const setCanvasRef = (el: Element | ComponentPublicInstance | null) => {
  canvasRef.value = el instanceof HTMLCanvasElement ? el : undefined
}
const setAudioRef = (el: Element | ComponentPublicInstance | null) => {
  audioRef.value = el instanceof HTMLAudioElement ? el : undefined
}
const setControlOverlayRef = (el: Element | ComponentPublicInstance | null) => {
  controlOverlayRef.value = el instanceof HTMLDivElement ? el : undefined
}

const sendNavAction = (action: Extract<ScreenMirrorControlAction, 'BACK' | 'HOME' | 'RECENTS' | 'LOCK_SCREEN'>) => {
  control.sendControl({ action })
}

const stopService = () => {
  if (recording.value) toggleRecording()
  service.stop()
}

const toggleMute = async () => {
  if (muted.value) {
    if (!pipeline.isAudioEnabled()) {
      try {
        await pipeline.enableAudio()
      } catch (e) {
        console.error('[ScreenMirrorView] enableAudio failed', e)
        return
      }
    }
    pipeline.setAudioMuted(false)
    muted.value = false
  } else {
    pipeline.setAudioMuted(true)
    muted.value = true
  }
}

watch(controlOverlayRef, (el) => {
  control.removeListeners()
  control.attachOverlay(el)
  if (el) control.setupListeners()
})

onActivated(() => {
  isActive.value = true
  emitter.on('screen_mirroring', service.onScreenMirroring)
  emitter.on('screen_mirror_video', pipeline.handleVideo)
  emitter.on('screen_mirror_audio', pipeline.handleAudio)
  emitter.on('screen_mirror_video_codec', pipeline.handleConfig)
  emitter.on('app_socket_connection_changed', service.onSocketReconnect)
  emitter.on('screen_mirror_audio_granted', service.onAudioGranted)
  document.addEventListener('fullscreenchange', media.onFullscreenChange)
  media.attachFullscreenListener()
  service.fetchState()
})

onDeactivated(() => {
  isActive.value = false
  emitter.off('screen_mirroring', service.onScreenMirroring)
  emitter.off('screen_mirror_video', pipeline.handleVideo)
  emitter.off('screen_mirror_audio', pipeline.handleAudio)
  emitter.off('screen_mirror_video_codec', pipeline.handleConfig)
  emitter.off('app_socket_connection_changed', service.onSocketReconnect)
  emitter.off('screen_mirror_audio_granted', service.onAudioGranted)
  document.removeEventListener('fullscreenchange', media.onFullscreenChange)
  media.detachFullscreenListener()
  pipeline.cleanup()
  service.deactivate()
})
</script>

<style lang="scss" src="@/styles/screen-mirror.scss"></style>
