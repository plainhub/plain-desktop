import { getCurrentDeviceHost } from '../device/current'
import { get as prefsGet } from '../prefs'
import { isMacPlatform } from '../platform'
import { DeviceType } from '../status'
import { applyScheme } from '../url'

export interface PendingLoginDevice {
  name: string
  host: string
  deviceType: DeviceType
}

// ─── Runtime context state ────────────────────────────────────────────────────

// Temporary target device set during the login flow (before the session is committed to the store).
// Allows api.ts to route requests to the target device before clientId is known.
let _pendingLoginDevice: PendingLoginDevice | null = null

export function setPendingLoginDevice(device: PendingLoginDevice): void { _pendingLoginDevice = device }
export function clearPendingLoginDevice(): void { _pendingLoginDevice = null }
export function getPendingLoginDevice(): PendingLoginDevice | null { return _pendingLoginDevice }

// Port of the local HTTP reverse proxy (Tauri only). Initialized at app start via
// invoke('http_proxy_port'). Used by file uploads so XHR progress events work and
// WKWebView doesn't reject the self-signed device certificate.
let _httpProxyPort = 0
export function setHttpProxyPort(port: number): void { _httpProxyPort = port }

let _localServerPort = 0
export function setLocalServerPort(port: number): void { _localServerPort = port }
export function getLocalServerPort(): number { return _localServerPort }

let _localToken = ''
export function setLocalServerToken(token: string): void { _localToken = token }
export function getLocalToken(): string { return _localToken }

let _localServerHttpsPort = 0
export function setLocalServerHttpsPort(port: number): void { _localServerHttpsPort = port }
export function getLocalServerHttpsPort(): number { return _localServerHttpsPort }

// ─── URL builders ─────────────────────────────────────────────────────────────

function isSecurePort(host: string): boolean {
  const m = host.match(/:(\d+)$/)
  return m ? m[1].endsWith('43') : false
}

/** Base URL for a device host (`ip:port`) — phones serve TLS on *43 ports. */
export function deviceBaseUrl(host: string): string {
  return applyScheme(isSecurePort(host) ? 'https' : 'http', host)
}

/** Routes a browser-initiated path through the local HTTP reverse proxy when
 *  the target base is a self-signed HTTPS device (Tauri only); the proxy
 *  learns the target from the `_pt` query param. */
export function proxyUrlFor(base: string, path: string): string {
  if (__IS_TAURI__ && base.startsWith('https://')) {
    const sep = path.includes('?') ? '&' : '?'
    return `http://127.0.0.1:${_httpProxyPort}${path}${sep}_pt=${encodeURIComponent(base)}`
  }
  return `${base}${path}`
}

export function getProxyUrl(path: string): string {
  return proxyUrlFor(getApiBaseUrl(), path)
}

export function getApiHost() {
  if (__IS_TAURI__) {
    const h = _pendingLoginDevice?.host || getCurrentDeviceHost()
    if (h) return h
  }
  return import.meta.env.VITE_APP_API_HOST || window.location.host
}

export function getApiBaseUrl() {
  if (__IS_TAURI__ && _localServerPort && !(_pendingLoginDevice || getCurrentDeviceHost())) {
    return `http://localhost:${_localServerPort}`
  }
  if (__IS_TAURI__ && (_pendingLoginDevice || getCurrentDeviceHost())) {
    return deviceBaseUrl(getApiHost())
  }
  return applyScheme(window.location.protocol.replace(':', ''), getApiHost())
}

export function getWebSocketBaseUrl() {
  if (__IS_TAURI__ && _localServerPort && !(_pendingLoginDevice || getCurrentDeviceHost())) {
    return `ws://localhost:${_localServerPort}`
  }
  if (__IS_TAURI__ && (_pendingLoginDevice || getCurrentDeviceHost())) {
    const p = isSecurePort(getApiHost()) ? 'wss' : 'ws'
    return `${p}://${getApiHost()}`
  }
  const p = window.location.protocol === 'http:' ? 'ws' : 'wss'
  return `${p}://${getApiHost()}`
}

export function getPhoneIp(): string {
  try {
    return new URL(`http://${getApiHost()}`).hostname
  } catch {
    return getApiHost().split(':')[0]
  }
}

// ─── Request headers ──────────────────────────────────────────────────────────

// Client self-identification for the `c-platform` header, mirroring
// plain-app's RequestHeaders (android/ios there). This describes the
// CLIENT runtime only — which server product we talk to is reported by
// the server's own APIs.
function clientPlatform(): string {
  if (!__IS_TAURI__) return 'web'
  if (isMacPlatform()) return 'macos'
  if (/Windows/i.test(navigator.userAgent)) return 'windows'
  return 'linux'
}

export function getApiHeaders() {
  const headers: Record<string, string> = {
    'Content-Type': 'multipart/form-data',
    'c-id': prefsGet('client_id', ''),
    'c-platform': clientPlatform(),
  }
  if (__APP_VERSION__) headers['c-version'] = __APP_VERSION__
  return headers
}
