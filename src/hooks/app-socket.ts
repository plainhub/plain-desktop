import { isLocalMode } from '@/lib/device/local-mode'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import emitter from '@/plugins/eventbus'
import toast from '@/components/toaster'
import { setLocalServerPort, setLocalServerHttpsPort } from '@/lib/api/api'
import { subscribeLoginPeerUpdates } from '@/lib/device/login-peers'
import { changeColor, getCurrentMode } from '@/lib/theme'
import { preload as preloadPrefs } from '@/lib/prefs/core'
import { apply as applyAppearance } from '@/lib/prefs/appearance'
import { readLocalServer } from '@/lib/api/local-server'


export function useAppSocket() {
  const { t } = useI18n()
  document.title = 'PlainApp'

  const wsStatus = ref('')
  const tapPhoneMessage = ref('')
  let clearStatusTimer: ReturnType<typeof setTimeout> | undefined
  let stopPeerUpdates: (() => void) | undefined
  let disposed = false
  let stopPrefsChanges: (() => void) | undefined
  let stopHttpStatus: (() => void) | undefined
  let stopConnection: (() => void) | undefined
  let prefsRefresh = Promise.resolve()

  const closeTapPhone = () => {
    tapPhoneMessage.value = ''
  }

  function handleOpen() {
    if (clearStatusTimer) clearTimeout(clearStatusTimer)
    clearStatusTimer = setTimeout(() => {
      wsStatus.value = ''
    }, 2000)
  }

  function setDisconnected() {
    if (clearStatusTimer) {
      clearTimeout(clearStatusTimer)
      clearStatusTimer = undefined
    }
    wsStatus.value = 'closed'
  }

  const stopSubscriptions: (() => void)[] = []
  onUnmounted(() => stopSubscriptions.forEach(stop => stop()))
  onMounted(() => {
    emitter.on('toast', (r: string) => toast(t(r), 'error'))
    emitter.on('tap_phone', (r: string) => {
      tapPhoneMessage.value = r
    })
    // A permission change on the phone resolves whatever "check phone" flow
    // the banner was pointing at (grant permission / toggle access switch).
    stopSubscriptions.push(graphqlSubscriptions.subscribe('accessUpdated', () => {
      tapPhoneMessage.value = ''
    }))
    stopSubscriptions.push(graphqlSubscriptions.subscribe('pairingCompleted', (result) => {
      emitter.emit('PAIRING_SUCCESS', { ...result, error: '' })
    }))
    stopSubscriptions.push(graphqlSubscriptions.subscribe('pairingRequested', (request) => emitter.emit('PAIRING_REQUEST_RECEIVED', request)))
    stopSubscriptions.push(graphqlSubscriptions.subscribe('pairingStarted', (result) => emitter.emit('PAIRING_STARTED', { ...result, error: result.error ?? '' })))
    stopSubscriptions.push(graphqlSubscriptions.subscribe('pairingFailed', (result) => emitter.emit('PAIRING_FAILED', { ...result, error: result.error ?? '' })))
    stopSubscriptions.push(graphqlSubscriptions.subscribe('pairingCanceled', (result) => emitter.emit('PAIRING_CANCELED', { ...result, error: result.error ?? '' })))
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
      if (getCurrentMode() !== 'auto') return
      changeColor()
    })
    try {
      changeColor()
    } catch (ex) {
      console.error(ex)
    }
    if (__IS_TAURI__) {
      void (async () => {
        const { Channel, invoke } = await import('@tauri-apps/api/core')
        const apply = (ports: { httpPort: number; httpsPort: number }) => {
          setLocalServerPort(ports.httpPort)
          setLocalServerHttpsPort(ports.httpsPort)
          emitter.emit('refetch_app')
        }
        let ready = false
        const buffered: Array<{ httpPort: number; httpsPort: number }> = []
        const channel = new Channel<{ state: string; httpPort: number; httpsPort: number }>()
        channel.onmessage = (payload) => {
          if (payload.state !== 'started') return
          if (ready) apply(payload)
          else buffered.push(payload)
        }
        const id = await invoke<number>('native_register_http_status', { channel })
        const stop = () => { void invoke('native_unregister_http_status', { id }).catch(console.error) }
        if (disposed) { stop(); return }
        stopHttpStatus = stop
        const current = await readLocalServer()
        if (disposed) return
        apply(current)
        buffered.forEach(apply)
        ready = true
      })().catch(console.error)
      void import('@tauri-apps/api/event').then(({ listen }) => listen('prefsChanged', () => {
        prefsRefresh = prefsRefresh.then(async () => {
          if (!isLocalMode()) return
          await preloadPrefs()
          await applyAppearance()
        }).catch(console.error)
      })).then((stop) => { if (disposed) stop(); else stopPrefsChanges = stop }).catch(console.error)
      void subscribeLoginPeerUpdates().then((stop) => {
        if (disposed) stop()
        else stopPeerUpdates = stop
      }).catch(console.error)
    }
    stopConnection = graphqlSubscriptions.onConnectionChange((connected) => {
      if (connected === null) wsStatus.value = ''
      else if (connected) handleOpen()
      else setDisconnected()
    })
  })

  onUnmounted(() => {
    disposed = true
    clearTimeout(clearStatusTimer)
    stopConnection?.()
    stopPeerUpdates?.()
    stopPrefsChanges?.()
    stopHttpStatus?.()
  })

  return { wsStatus, tapPhoneMessage, closeTapPhone }
}
