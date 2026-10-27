import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import emitter from '@/plugins/eventbus'
import toast from '@/components/toaster'
import { getWebSocketBaseUrl, getLocalToken, isNativeTarget, setLocalServerPort, setLocalServerHttpsPort } from '@/lib/api/api'
import { projectPeerNotificationEvent } from '@/lib/peer/peer-events'
import { subscribeLoginPeerUpdates } from '@/lib/device/login-peers'
import { chachaDecrypt, chachaEncrypt, bitArrayToUint8Array } from '@/lib/api/crypto'
import { parseWebSocketData } from '@/lib/api/sjcl-arraybuffer'
import { EventStream, eventAad } from '@/lib/api/websocket-events'
import { applyDarkClass, changeColor, changeColorMode, getCurrentMode, getLastSavedAutoColorMode, isModeDark } from '@/lib/theme'
import { tokenToKey } from '@/lib/api/file'
import { getRemoteClientId } from '@/lib/device/client-id'
import { getCurrentAuthToken } from '@/lib/device/current'
import { isLocalMode } from '@/lib/device/local-mode'
import { openSocket } from '@/lib/api/http'
import { subscribeNativeEvents, type NativeEventStream } from '@/lib/api/native-events'
import { get as prefsGet, set as prefsSet, applySnapshot } from '@/lib/prefs'
import { nativeGraphql } from '@/lib/api/native-client'

const RAW_BINARY_EVENTS = new Set(['SCREEN_MIRROR_VIDEO', 'SCREEN_MIRROR_AUDIO', 'IMAGE_EDITOR_UPDATE'])

export function useAppSocket() {
  const { t } = useI18n()
  document.title = 'PlainApp'

  const wsStatus = ref('')
  const tapPhoneMessage = ref('')
  let retryConnectTimeout: ReturnType<typeof setTimeout> | undefined
  let clearStatusTimer: ReturnType<typeof setTimeout> | undefined
  let retryTime = 1000
  let nativeStream: NativeEventStream | null = null
  let stopPeerUpdates: (() => void) | undefined
  let disposed = false
  let stopPrefsChanges: (() => void) | undefined
  let prefsRefresh = Promise.resolve()

  const closeTapPhone = () => {
    tapPhoneMessage.value = ''
  }

  function dispatchEvent(r: { type: string; data: Uint8Array }, key: Uint8Array, native: boolean) {
    try {
      const type = r.type
      if (RAW_BINARY_EVENTS.has(r.type)) {
        emitter.emit(type as any, r.data)
      } else {
        const json = native ? chachaDecrypt(key, r.data, eventAad(type)) : new TextDecoder().decode(r.data)
        const data = json ? JSON.parse(json) : null
        if (type === 'PEER_NOTIFICATIONS_UPDATED') {
          projectPeerNotificationEvent(data, native)
          return
        }
        // Enabled automatically in dev mode (see main.ts)
        if (window.__PLAIN_LOG__) console.log(`[ws event] ${type}`, data)
        emitter.emit(type as any, data)
      }
    } catch (ex) {
      console.error(ex)
    }
  }

  function handleOpen() {
    emitter.emit('app_socket_connection_changed', true)
    retryTime = 1000
    if (clearStatusTimer) clearTimeout(clearStatusTimer)
    clearStatusTimer = setTimeout(() => {
      wsStatus.value = ''
    }, 2000)
  }

  function handleDrop() {
    if (clearStatusTimer) {
      clearTimeout(clearStatusTimer)
      clearStatusTimer = undefined
    }
    wsStatus.value = 'closed'
    retryConnect()
  }

  /** Local target: the shared Rust core's own event feed, not a socket. */
  function connectNative() {
    const token = getLocalToken()
    if (!token) return
    const key = tokenToKey(token)
    nativeStream?.close()
    nativeStream = subscribeNativeEvents(
      (frame) => {
        try { dispatchEvent(parseWebSocketData(frame), key, true) } catch (error) { console.error(error) }
      },
      () => {
        nativeStream = null
        handleDrop()
      }
    )
    handleOpen()
  }

  async function connect() {
    if (isNativeTarget()) {
      connectNative()
      return
    }
    const clientId = prefsGet('client_id', '')
    const localMode = isLocalMode()
    const token = localMode ? getLocalToken() : getCurrentAuthToken()
    if (!token) {
      wsStatus.value = ''
      return
    }

    try {
      const key = tokenToKey(token)
      const wsUrl = `${getWebSocketBaseUrl()}?cid=${clientId}`
      const socket = openSocket(wsUrl, localMode ? '' : getRemoteClientId())
      socket.binaryType = 'arraybuffer'
      let stream: EventStream | null = null
      socket.onopen = () => {
        const registration = bitArrayToUint8Array(chachaEncrypt(key, Date.now().toString()))
        stream = new EventStream(registration)
        handleOpen()
        socket.send(registration)
      }
      socket.onmessage = (event: MessageEvent) => {
        let packet: { type: string; data: Uint8Array }
        try {
          if (!stream || !(event.data instanceof ArrayBuffer)) throw new Error('Invalid event frame')
          packet = stream.decode(event.data, key)
        } catch (error) {
          console.error(error)
          stream = null
          socket.close(1008, 'invalid_event')
          return
        }
        dispatchEvent(packet, key, false)
      }
      socket.onclose = () => {
        handleDrop()
      }
      socket.onerror = () => {
        wsStatus.value = 'error'
        socket.close()
        emitter.emit('app_socket_connection_changed', false)
      }
    } catch (ex) {
      console.error(ex)
      retryConnect()
    }
  }

  function retryConnect() {
    if (retryConnectTimeout) clearTimeout(retryConnectTimeout)
    retryConnectTimeout = setTimeout(() => connect(), Math.min(5000, retryTime))
    retryTime += 1000
  }

  function initializeTheme() {
    applyDarkClass(isModeDark(getCurrentMode() || 'auto', false))
    if (getCurrentMode() === 'auto') {
      const actual = isModeDark('auto', false) ? 'dark' : 'light'
      if (actual !== getLastSavedAutoColorMode()) changeColorMode('auto')
    }
  }

  const stopSubscriptions: (() => void)[] = []
  onUnmounted(() => stopSubscriptions.forEach(stop => stop()))
  onMounted(() => {
    emitter.on('HTTP_SERVER_STARTED', (started) => {
      setLocalServerPort(started.httpPort)
      setLocalServerHttpsPort(started.httpsPort)
      emitter.emit('refetch_app')
    })
    emitter.on('toast', (r: string) => toast(t(r), 'error'))
    emitter.on('tap_phone', (r: string) => {
      tapPhoneMessage.value = r
    })
    // A permission change on the phone resolves whatever "check phone" flow
    // the banner was pointing at (grant permission / toggle access switch).
    stopSubscriptions.push(graphqlSubscriptions.subscribe('permissionsUpdated', () => {
      tapPhoneMessage.value = ''
    }))
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
      if (getCurrentMode() !== 'auto') return
      changeColor()
    })
    try {
      initializeTheme()
    } catch (ex) {
      console.error(ex)
    }
    if (__IS_TAURI__) {
      void import('@tauri-apps/api/event').then(({ listen }) => listen('prefsChanged', () => {
        prefsRefresh = prefsRefresh.then(async () => {
          const result = await nativeGraphql<{ data?: { prefs: Record<string, unknown> }; errors?: { message: string }[] }>({ query: 'query { prefs }' }, 30000)
          if (result.errors?.length) throw new Error(result.errors[0].message)
          if (result.data) await applySnapshot(result.data.prefs)
          if (isLocalMode()) emitter.emit('PREFS_UPDATED')
        }).catch(console.error)
      })).then((stop) => { if (disposed) stop(); else stopPrefsChanges = stop }).catch(console.error)
      void subscribeLoginPeerUpdates().then((stop) => {
        if (disposed) stop()
        else stopPeerUpdates = stop
      }).catch(console.error)
    }
    connect()
  })

  onUnmounted(() => {
    disposed = true
    stopPeerUpdates?.()
    stopPrefsChanges?.()
  })

  return { wsStatus, tapPhoneMessage, closeTapPhone }
}
