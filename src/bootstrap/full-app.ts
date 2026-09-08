import { createApp } from 'vue'
import App from '@/App.vue'
import router from '@/plugins/router'
import '@/styles/main.scss'
import { createPinia } from 'pinia'
import VueClickAway from '@/plugins/clickaway'
import VueTooltip from '@/plugins/tooltip'
import VueRipple from '@/plugins/ripple'
import { shortUUID } from '@/lib/strutil'
import { getIsPhone, getIsTablet } from '@/hooks/device'
import { setHttpProxyPort, setLocalServerPort, setLocalServerToken, setLocalServerHttpsPort } from '@/lib/api/api'
import { preload as preloadPrefs, get as prefsGet, set as prefsSet } from '@/lib/prefs'
import { applyUrlClientId } from '@/lib/device/client-id'
import { preloadLoginPeers } from '@/lib/device/login-peers'
import { applyMenuLabels } from '@/lib/app-menu'
import { preventUnhandledFileDrop } from '@/lib/file-drop'

if (!__IS_TAURI__) {
  // Web: prefs are synchronous (localStorage) — safe to apply immediately.
  // Pull `?__cid=` (set when spawning child windows) into sessionStorage
  // before anything else reads the clientId. Idempotent.
  applyUrlClientId()
  import('@/registerServiceWorker')
  // Web mode: ensure client_id exists in localStorage on startup
  if (!prefsGet('client_id', '')) {
    prefsSet('client_id', shortUUID())
  }
}

export async function bootstrapFullApp(): Promise<void> {
  if (__IS_TAURI__) {
    document.documentElement.classList.add('tauri')
    preventUnhandledFileDrop()
    // Load all prefs from plugin-store into memory FIRST so synchronous
    // readers (i18n, stores) see the persisted values without hitting localStorage.
    await preloadPrefs()
    // Ensure client_id exists in prefs for Tauri mode.
    if (!prefsGet('client_id', '')) {
      prefsSet('client_id', shortUUID())
    }
    // Pull `?__cid=` (set by `openWindow` when spawning child windows) into
    // sessionStorage. Must run AFTER preloadPrefs(): the local-vs-device
    // check compares the cid against the persisted desktop client_id, which
    // reads as empty before the prefs cache is populated — a child window
    // would then drop its device binding and fall back to the local server
    // (e.g. /fs requests hit http://localhost:<port> and fail with 401).
    // Idempotent.
    applyUrlClientId()
    const { invoke } = await import('@tauri-apps/api/core')
    await Promise.all([
      invoke<number>('http_proxy_port').then(setHttpProxyPort),
      invoke<number>('local_server_port').then(setLocalServerPort),
      invoke<number>('local_server_https_port').then(setLocalServerHttpsPort),
      invoke<string>('local_server_token').then(setLocalServerToken),
      preloadLoginPeers(),
    ])
  }

  // Dynamically import i18n AFTER prefs are loaded so createI18n() reads
  // the persisted locale from the prefs cache instead of localStorage.
  const i18nModule = await import('@/plugins/i18n')
  const i18n = i18nModule.default
  const { loadLocaleMessages, syncLocaleFromPrefs } = i18nModule

  // The initialLocale IIFE inside i18n.ts reads from the in-memory cache at
  // module-import time. As a safety net, re-verify the persisted locale
  // here and correct it before any locale-dependent code runs.
  const locale = syncLocaleFromPrefs()

  // Pre-load the persisted locale's chunk synchronously so the first
  // render shows the right translations instead of the en-US fallback.
  // Other locales stay on disk until the user switches to them.
  const { setLocale: setTimeagoLocale } = await import('@/lib/timeago')
  await Promise.all([loadLocaleMessages(locale), setTimeagoLocale(locale)])

  if (__IS_TAURI__) {
    applyMenuLabels()
  }

  createApp(App).use(VueClickAway).use(VueTooltip).use(VueRipple).use(createPinia()).use(router).use(i18n).provide('isPhone', getIsPhone()).provide('isTablet', getIsTablet()).mount('#app')

  // Pre-warm the media preview window 2s after the main window finishes
  // mounting. The delay keeps the cold-start critical path snappy and
  // moves the cost (creating a hidden webview, loading the SPA once
  // inside it) out of the user's first click.
  if (__IS_TAURI__) {
    setTimeout(() => {
      import('@tauri-apps/api/core').then(({ invoke }) => {
        invoke('media_preview_init').catch((e) => {
          console.warn('media_preview_init failed', e)
        })
      })
    }, 2000)
  }
}
