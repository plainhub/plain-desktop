import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import tapPhone from '@/plugins/tapphone'
import emitter from '@/plugins/eventbus'
import { initLazyQuery, screenMirrorControlEnabledGQL, isScreenMirroringGQL } from '@/lib/api/query'
import {
  initMutation, relaunchAppGQL, startScreenMirrorGQL, stopScreenMirrorGQL,
  updateScreenMirrorQualityGQL, requestScreenMirrorAudioGQL,
} from '@/lib/api/mutation'
import type { GqlError } from '@/lib/api/gql-client'
import { openModal } from '@/components/modal'
import AccessibilityGuideModal from '@/components/AccessibilityGuideModal.vue'

export type MirrorState = 'idle' | 'requesting' | 'connecting' | 'streaming' | 'failed'

export function useScreenMirrorService() {
  const { t } = useI18n()
  const state = ref<MirrorState>('idle')
  const seconds = ref(0)
  const qualityMode = ref('HD')
  const controlEnabled = ref(false)
  const audioRequesting = ref(false)
  let accessibilityEnabled = false
  let countdownTimer: ReturnType<typeof setInterval>
  let connectFn: () => void = () => {}
  let cleanupFn: () => void = () => {}
  let retryCount = 0
  let retryTimer: ReturnType<typeof setTimeout>
  let relaunchPending = false

  const clearCountdown = () => { clearInterval(countdownTimer); seconds.value = 0 }
  const setPipeline = (connect: () => void, cleanup: () => void) => { connectFn = connect; cleanupFn = cleanup }

  const fullReset = () => {
    state.value = 'idle'; clearCountdown(); clearTimeout(retryTimer)
    controlEnabled.value = false; retryCount = 0
  }

  // --- State query (lazy: refetched on reactivation) ---
  const { fetch: fetchState, loading: fetchStateLoading } = initLazyQuery({
    handle: (data: any, error: string) => {
      if (error) {
        toast(t(error), 'error')
        if (state.value === 'streaming' || state.value === 'connecting') state.value = 'failed'
        return
      }
      if (data?.screenMirrorQuality?.mode) qualityMode.value = data.screenMirrorQuality.mode
      accessibilityEnabled = data?.screenMirrorControlEnabled === true
      if (data?.isScreenMirroring) {
        if (state.value === 'idle' || state.value === 'failed') { state.value = 'connecting'; retryCount = 0; connectFn() }
      } else {
        if (state.value === 'connecting' || state.value === 'streaming') cleanupFn()
        state.value = 'idle'
      }
    },
    document: isScreenMirroringGQL,
    variables: () => ({}),
    options: { fetchPolicy: 'no-cache' },
  })

  // --- Mutations ---
  const { mutate: startMirror, loading: startLoading, onDone: onStartDone, onError: onStartError } = initMutation({ document: startScreenMirrorGQL }, false)
  const { mutate: stopMirror, loading: stopLoading, onDone: onStopDone, onError: onStopError } = initMutation({ document: stopScreenMirrorGQL })
  const { mutate: updateQuality, onDone: onQualityDone } = initMutation({ document: updateScreenMirrorQualityGQL })
  const { mutate: requestAudio, onDone: onAudioDone, onError: onAudioError } = initMutation({ document: requestScreenMirrorAudioGQL }, false)
  const { mutate: relaunchMutate, loading: relaunchLoading, onError: onRelaunchError } = initMutation({ document: relaunchAppGQL })

  // Start / Stop
  const start = () => { state.value = 'requesting'; startMirror({ audio: true }) }
  onStartDone(() => {
    seconds.value = 30
    countdownTimer = setInterval(() => {
      if (--seconds.value <= 0) { state.value = 'failed'; clearInterval(countdownTimer) }
    }, 1000)
  })
  onStartError((e: GqlError) => { toast(t(e.message), 'error'); state.value = 'failed' })
  const stop = () => stopMirror()
  onStopDone(() => { fullReset(); cleanupFn() })
  onStopError((e: GqlError) => toast(t(e.message), 'error'))

  // Quality
  let pendingMode: string | null = null
  const setQualityMode = (mode: string) => { pendingMode = mode; updateQuality({ mode }) }
  onQualityDone(() => { if (pendingMode) { qualityMode.value = pendingMode; pendingMode = null } })

  // Audio permission
  const requestAudioPermission = () => {
    if (audioRequesting.value) return
    audioRequesting.value = true
    tapPhone(t('confirm_mirror_audio_permission_on_phone'))
    requestAudio()
  }
  onAudioDone((r: any) => {
    if (r?.data?.requestScreenMirrorAudio) { audioRequesting.value = false; tapPhone(''); emitter.emit('refetch_app') }
  })
  onAudioError((e: GqlError) => { audioRequesting.value = false; tapPhone(''); toast(t(e.message), 'error') })

  // Relaunch
  const relaunchApp = () => { relaunchPending = true; fullReset(); cleanupFn(); relaunchMutate() }
  onRelaunchError((e: GqlError) => { relaunchPending = false; toast(t(e.message), 'error') })

  // Control toggle
  const { fetch: fetchControlEnabled } = initLazyQuery({
    handle: (data: any) => {
      accessibilityEnabled = data?.screenMirrorControlEnabled === true
      if (accessibilityEnabled) controlEnabled.value = true
    },
    document: screenMirrorControlEnabledGQL,
    variables: () => ({}),
  })
  const toggleControl = () => {
    if (controlEnabled.value) { controlEnabled.value = false; return }
    if (!accessibilityEnabled) {
      openModal(AccessibilityGuideModal, { onConfirm: () => fetchControlEnabled() })
      return
    }
    controlEnabled.value = true
  }

  const showLoading = computed(() => fetchStateLoading.value || startLoading.value || relaunchLoading.value || stopLoading.value)

  // --- Pipeline callbacks ---
  const onStreamReady = () => { state.value = 'streaming'; clearCountdown(); retryCount = 0 }
  const onDisconnected = () => {
    if (state.value === 'streaming' && retryCount < 3) {
      retryCount++; state.value = 'connecting'
      retryTimer = setTimeout(() => connectFn(), retryCount * 1500)
    } else if (state.value !== 'idle') { state.value = 'failed'; retryCount = 0 }
  }
  const onScreenMirrorOff = () => { fullReset() }

  // --- Event handlers ---
  const onScreenMirroring = (data: any) => {
    if (data?.running === false) {
      // Screen mirror stopped on the phone — reset to idle so the start
      // button reappears. Without this the web page stays stuck in
      // streaming/connecting because no video frames arrive.
      if (state.value !== 'idle') { cleanupFn(); fullReset() }
      return
    }
    if (state.value === 'streaming' || state.value === 'connecting') return
    clearCountdown(); state.value = 'connecting'; retryCount = 0; connectFn()
  }
  const deactivate = () => { state.value = 'idle'; clearCountdown(); clearTimeout(retryTimer) }
  const onSocketReconnect = (c: boolean) => { if (c && relaunchPending) { relaunchPending = false; start() } }
  const onAudioGranted = () => window.location.reload()

  return {
    state, seconds, qualityMode, controlEnabled, audioRequesting,
    showLoading, relaunchLoading, stopLoading, setPipeline, fetchState, start, stop,
    setQualityMode, requestAudioPermission, relaunchApp, toggleControl,
    onStreamReady, onDisconnected, onScreenMirrorOff, onScreenMirroring, onSocketReconnect, onAudioGranted, deactivate,
  }
}
