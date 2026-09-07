import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import emitter from '@/plugins/eventbus'
import toast from '@/components/toaster'
import { getWebSocketBaseUrl, getLocalToken } from '@/lib/api/api'
import { preloadLoginPeers } from '@/lib/device/login-peers'
import { chachaDecrypt, chachaEncrypt, bitArrayToUint8Array } from '@/lib/api/crypto'
import { parseWebSocketData } from '@/lib/api/sjcl-arraybuffer'
import { applyDarkClass, changeColor, changeColorMode, getCurrentMode, getLastSavedAutoColorMode, isModeDark } from '@/lib/theme'
import { tokenToKey } from '@/lib/api/file'
import { getRemoteClientId } from '@/lib/device/client-id'
import { getCurrentAuthToken } from '@/lib/device/current'
import { isLocalMode } from '@/lib/device/local-mode'
import { openSocket } from '@/lib/api/http'
import { get as prefsGet, set as prefsSet } from '@/lib/prefs'

const EventType: { [key: number]: string } = {
  1: 'message_created',
  2: 'message_deleted',
  3: 'message_updated',
  4: 'feeds_fetched',
  5: 'screen_mirroring',
  7: 'notification_created',
  8: 'notification_updated',
  9: 'notification_deleted',
  10: 'notification_refreshed',
  11: 'pomodoro_action',
  12: 'pomodoro_settings_update',
  14: 'screen_mirror_audio_granted',
  15: 'bookmark_updated',
  16: 'download_progress',
  17: 'mms_sent',
  18: 'channels_updated',
  19: 'image_search_updated',
  20: 'peer_status_updated',
  21: 'device_name_updated',
  22: 'pairing_request_received',
  23: 'pairing_success',
  24: 'pairing_failed',
  25: 'pairing_canceled',
  26: 'pairing_started',
  27: 'nearby_device_found',
  28: 'channel_invite_received',
  29: 'nearby_discovery_started',
  30: 'nearby_discovery_stopped',
  31: 'screen_mirror_video',
  32: 'screen_mirror_video_codec',
  33: 'screen_mirror_audio',
  34: 'image_editor_update',
  35: 'sms_changed',
  36: 'sms_send_result',
  37: 'mms_send_result',
}

// Screen mirror binary frames (H.264 NAL / Opus) and image editor Yjs updates
// are sent raw — skip ChaCha20 decryption so the bytes are consumed directly.
const RAW_BINARY_EVENTS = new Set([
  31, // SCREEN_MIRROR_VIDEO
  33, // SCREEN_MIRROR_AUDIO
  34, // IMAGE_EDITOR_UPDATE
])

export function useAppSocket() {
  const { t } = useI18n()
  document.title = 'PlainApp'

  const wsStatus = ref('')
  const tapPhoneMessage = ref('')
  let retryConnectTimeout: ReturnType<typeof setTimeout> | undefined
  let clearStatusTimer: ReturnType<typeof setTimeout> | undefined
  let ws: WebSocket
  let retryTime = 1000

  const closeTapPhone = () => {
    tapPhoneMessage.value = ''
  }

  /**
   * App WS dropped after a successful dial — likely the device changed IPs.
   * Ask the local mDNS responder for a one-shot browse: the resident listener
   * refreshes the peer's ip:port in the peers table, and TauriWebSocket's
   * _resolveUrl reads that fresh host on the next connect().
   */
  const triggerMdnsBrowse = () => {
    if (!__IS_TAURI__) return
    invoke('mdns_browse').catch(() => {
      // ignore — browse is best-effort; retry will proceed without it
    })
  }

  async function connect() {
    const clientId = prefsGet('client_id', '')
    const localMode = isLocalMode()
    const token = localMode ? getLocalToken() : getCurrentAuthToken()
    if (!token) {
      return
    }

    try {
      const key = tokenToKey(token)
      const wsUrl = `${getWebSocketBaseUrl()}/?cid=${clientId}`
      ws = openSocket(wsUrl, localMode ? '' : getRemoteClientId())
      ws.onopen = async () => {
        emitter.emit('app_socket_connection_changed', true)
        retryTime = 1000
        ws.send(bitArrayToUint8Array(chachaEncrypt(key, new Date().getTime().toString())))
        if (clearStatusTimer) clearTimeout(clearStatusTimer)
        clearStatusTimer = setTimeout(() => {
          wsStatus.value = ''
        }, 2000)
      }
      ws.onmessage = async (event: MessageEvent) => {
        const buffer = await event.data.arrayBuffer()
        const r = parseWebSocketData(buffer)
        const type = EventType[r.type]
        try {
          if (RAW_BINARY_EVENTS.has(r.type)) {
            // Zero-copy: pass the Uint8Array view directly. The view shares
            // the underlying WebSocket buffer — no slice, no memcpy. Downstream
            // (mirror-codec-video / mirror-codec-audio) consumes Uint8Array
            // and never touches .buffer, so the 4-byte type prefix is never
            // read. This matters for 1080p@60fps where each frame is ~100KB
            // and a per-frame slice would copy ~6MB/s.
            emitter.emit(type as any, r.data)
          } else {
            const json = chachaDecrypt(key, r.data)
            const data = json ? JSON.parse(json) : null
            // Opt-in via DevTools (`__PLAIN_LOG__ = true`)
            if (window.__PLAIN_LOG__) console.log(`[ws event] ${type}`, data)
            emitter.emit(type as any, data)
          }
        } catch (ex) {
          console.error(ex)
        }
      }
      ws.onclose = () => {
        if (clearStatusTimer) {
          clearTimeout(clearStatusTimer)
          clearStatusTimer = undefined
        }
        wsStatus.value = 'closed'
        triggerMdnsBrowse()
        retryConnect()
      }
      ws.onerror = () => {
        wsStatus.value = 'error'
        ws.close()
        emitter.emit('app_socket_connection_changed', false)
      }
    } catch (ex) {
      console.error(ex)
      triggerMdnsBrowse()
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

  onMounted(() => {
    emitter.on('toast', (r: string) => toast(t(r), 'error'))
    emitter.on('tap_phone', (r: string) => {
      tapPhoneMessage.value = r
    })
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
      // Rust-side centralized host updates: the resident mDNS listener emits
      // this (only on real changes) after upserting the peers table. Re-pull
      // the login-peer mirror so every consumer (API base URL, WS dial,
      // switcher UI) sees the fresh address.
      void listen('device-host-changed', () => {
        void preloadLoginPeers()
      })
    }
    connect()
  })

  return { wsStatus, tapPhoneMessage, closeTapPhone }
}
