import {
  getRemoteClientId,
  getActiveClientId,
  clearRemoteClientId,
} from '@/lib/device/client-id'
import { findLoginPeer, clearLoginPeer, peerHost } from '@/lib/device/login-peers'

/** Current remote-device host (`ip:port`). Web mode has no stored host —
 *  requests target `window.location.host` via `getApiHost`'s fallback. */
export function getCurrentDeviceHost(): string {
  const peer = findLoginPeer(getRemoteClientId())
  return peer ? peerHost(peer) : ''
}

export function getCurrentDeviceBaseUrl(): string {
  return findLoginPeer(getRemoteClientId())?.address?.baseUrl || ''
}

/** Current login token: Tauri reads it from the peers table mirror,
 *  web reads the single `auth_token` from localStorage. */
export function getCurrentAuthToken(): string {
  if (!__IS_TAURI__) return localStorage.getItem('auth_token') ?? ''
  return findLoginPeer(getRemoteClientId())?.token ?? ''
}

/** Drops the login of the bound device (401 handler). Keeps the peer row. */
export function clearCurrentSession(): Promise<void> {
  const cleared = clearLoginPeer(getRemoteClientId())
  clearRemoteClientId()
  return cleared
}

export function getMainStateKey(): string {
  const id = getActiveClientId()
  return id ? `main_state:${id}` : 'main_state'
}
