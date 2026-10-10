import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import emitter from '@/plugins/eventbus'
import toast from '@/components/toaster'
import { getWebSocketBaseUrl, getLocalToken } from '@/lib/api/api'
import { preloadLoginPeers } from '@/lib/device/login-peers'
import { chachaDecrypt, chachaEncrypt, chachaEncryptBytes, bitArrayToUint8Array } from '@/lib/api/crypto'
import { parseWebSocketData } from '@/lib/api/sjcl-arraybuffer'
import { applyDarkClass, changeColor, changeColorMode, getCurrentMode, getLastSavedAutoColorMode, isModeDark } from '@/lib/theme'
import { tokenToKey } from '@/lib/api/file'
import { getRemoteClientId } from '@/lib/device/client-id'
import { getCurrentAuthToken } from '@/lib/device/current'
import { isLocalMode } from '@/lib/device/local-mode'
import { openSocket } from '@/lib/api/http'
import { get as prefsGet, set as prefsSet } from '@/lib/prefs'

const RAW_BINARY_EVENTS = new Set(['SCREEN_MIRROR_VIDEO', 'SCREEN_MIRROR_AUDIO', 'IMAGE_EDITOR_UPDATE'])

// Upstream control channel on the live app socket. Post-registration frames
// are ChaCha20-encrypted with the same key as the registration frame; the
// phone branches on the decrypted payload's first byte (0x54 = binary touch
// frame, otherwise JSON control input). See shared WebSocketRoutes.kt.
let controlSocket: WebSocket | null = null
let controlKey: Uint8Array | null = null

function bindControlChannel(ws: WebSocket, key: Uint8Array) {
  controlSocket = ws
  controlKey = key
}

function unbindControlChannel(ws: WebSocket) {
  if (controlSocket === ws) {
    controlSocket = null
    controlKey = null
  }
}

/** Send one encrypted binary frame upstream; false when the socket is down. */
export function sendAppWsBytes(plain: Uint8Array): boolean {
  const key = controlKey
  if (!controlSocket || !key) return false
  controlSocket.send(chachaEncryptBytes(key, plain))
  return true
}

/** Send one encrypted JSON payload upstream; false when the socket is down. */
export function sendAppWsJson(payload: unknown): boolean {
  return sendAppWsBytes(new TextEncoder().encode(JSON.stringify(payload)))
}

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
   * refreshes the peer's ip:port in the peers table, and the proxy's `_cid`
   * resolution reads that fresh host on the next connect().
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
      const wsUrl = `${getWebSocketBaseUrl()}?cid=${clientId}`
      ws = openSocket(wsUrl, localMode ? '' : getRemoteClientId())
      ws.onopen = async () => {
        emitter.emit('app_socket_connection_changed', true)
        retryTime = 1000
        ws.send(bitArrayToUint8Array(chachaEncrypt(key, new Date().getTime().toString())))
        // Control frames may only follow the registration frame
        bindControlChannel(ws, key)
        if (clearStatusTimer) clearTimeout(clearStatusTimer)
        clearStatusTimer = setTimeout(() => {
          wsStatus.value = ''
        }, 2000)
      }
      ws.onmessage = async (event: MessageEvent) => {
        const buffer = await event.data.arrayBuffer()
        try {
          const r = parseWebSocketData(buffer)
          const type = r.type
          if (RAW_BINARY_EVENTS.has(r.type)) {
            emitter.emit(type as any, r.data)
          } else {
            const json = chachaDecrypt(key, r.data)
            const data = json ? JSON.parse(json) : null
            // Enabled automatically in dev mode (see main.ts)
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
        unbindControlChannel(ws)
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
    // A permission change on the phone resolves whatever "check phone" flow
    // the banner was pointing at (grant permission / toggle access switch).
    emitter.on('PERMISSIONS_UPDATED', () => {
      tapPhoneMessage.value = ''
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
