import {
  getRemoteClientId,
  getActiveClientId,
  isLocalMode as _isLocalMode,
  clearRemoteClientId,
} from '@/lib/device/client-id'
import { findLoginPeer, clearLoginPeer, peerHost } from '@/lib/device/login-peers'

export { _isLocalMode as isLocalMode }

/** Current remote-device host (`ip:port`). Web mode has no stored host —
 *  requests target `window.location.host` via `getApiHost`'s fallback. */
export function getCurrentDeviceHost(): string {
  if (!__IS_TAURI__) return ''
  const peer = findLoginPeer(getRemoteClientId())
  return peer ? peerHost(peer) : ''
}

export function getCurrentDeviceBaseUrl(): string {
  return __IS_TAURI__ ? findLoginPeer(getRemoteClientId())?.address?.baseUrl || '' : ''
}

/** Current login token: Tauri reads it from the peers table mirror,
 *  web reads the single `auth_token` from localStorage. */
export function getCurrentAuthToken(): string {
  if (!__IS_TAURI__) return localStorage.getItem('auth_token') ?? ''
  return findLoginPeer(getRemoteClientId())?.token ?? ''
}

/** Drops the login of the bound device (401 handler). Keeps the peer row. */
export function clearCurrentSession(): void {
  if (!__IS_TAURI__) {
    localStorage.removeItem('auth_token')
  } else {
    void clearLoginPeer(getRemoteClientId())
  }
  clearRemoteClientId()
}

export function getMainStateKey(): string {
  const id = getActiveClientId()
  return id ? `main_state:${id}` : 'main_state'
}
