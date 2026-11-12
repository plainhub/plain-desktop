import { ResourcePurpose } from './aad-purpose.generated'
import { isLocalMode } from '@/lib/device/local-mode'
import { getCurrentDeviceHost, getCurrentDeviceBaseUrl } from '../device/current'
import { getDesktopClientId } from '@/lib/device/client-id'
import { isMacPlatform } from '../platform'
import { DeviceType } from '../status'
import { applyScheme, peerProxyPath } from '../url'
import { encryptResourcePayload } from './resource-link'

export interface PendingLoginDevice {
  id?: string
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

let _localServerPort = 0
export function setLocalServerPort(port: number): void { _localServerPort = port }
export function getLocalServerPort(): number { return _localServerPort }

let _urlToken = ''
export function setUrlToken(token: string): void {
  if (_urlToken !== token) proxyTargets.clear()
  _urlToken = token
}
export function getUrlToken(): string { return _urlToken }

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

const proxyTargets = new Map<string, string>()

function proxyPath(path: string, base: string): string {
  const descriptor = JSON.stringify({ base })
  let target = proxyTargets.get(descriptor)
  if (!target) {
    const key = Uint8Array.from(atob(_urlToken), byte => byte.charCodeAt(0))
    target = encryptResourcePayload(key, { mode: 'c', id: getDesktopClientId() }, ResourcePurpose.Fs, descriptor)
    if (proxyTargets.size >= 64) proxyTargets.clear()
    proxyTargets.set(descriptor, target)
  }
  return localProxyUrl(target, path)
}

function localProxyUrl(target: string, path: string, socket = false): string {
  return `${socket ? 'ws' : 'http'}://127.0.0.1:${_localServerPort}${peerProxyPath(target, path)}`
}

export function proxyUrlFor(base: string, path: string): string {
  return __IS_TAURI__ && base.startsWith('https://')
    ? proxyPath(path, new URL(base).origin)
    : `${base}${path}`
}

export function proxyHttpUrl(url: string): string {
  if (!__IS_TAURI__ || !url.startsWith('https://')) return url
  const { origin, pathname, search } = new URL(url)
  return proxyPath(pathname + search, origin)
}

export function proxyWsUrlFor(url: string, clientId = ''): string {
  if (!__IS_TAURI__) return url
  const u = new URL(url)
  if (u.hostname === 'localhost' || u.hostname === '127.0.0.1' || u.hostname === '[::1]') return url
  return clientId ? localProxyUrl(clientId, u.pathname + u.search, true) : url
}

export function getProxyUrl(path: string): string {
  return proxyUrlFor(getApiBaseUrl(), path)
}

export function getApiHost() {
  return _pendingLoginDevice?.host || getCurrentDeviceHost() || (import.meta.env.DEV ? import.meta.env.VITE_APP_API_HOST : '') || window.location.host
}

export function getApiBaseUrl() {
  const saved = !_pendingLoginDevice && getCurrentDeviceBaseUrl()
  if (saved) return saved
  if (isLocalMode() && !_pendingLoginDevice && _localServerPort) {
    return `http://localhost:${_localServerPort}`
  }
  const host = getApiHost()
  const scheme = window.location.protocol === 'https:' || isSecurePort(host) ? 'https' : 'http'
  return applyScheme(scheme, host)
}

export function getWebSocketBaseUrl() {
  return getApiBaseUrl().replace(/^http/, 'ws')
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
    'c-id': getDesktopClientId(),
    'c-platform': clientPlatform(),
  }
  if (__APP_VERSION__) headers['c-version'] = __APP_VERSION__
  return headers
}
