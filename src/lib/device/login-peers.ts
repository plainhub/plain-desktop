import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { IPeer } from '@/lib/interfaces'
import { DeviceType } from '@/lib/status'

const AUTH_TOKEN_KEY = 'auth_token'

export type LoginPeer = IPeer & { token: string; publicKey: string }

export interface SaveLoginPeerInput {
  clientId: string
  name: string
  host: string
  deviceType: DeviceType
  token: string
  signaturePublicKey: string
  chatKey?: string
}

/** First IP from a comma-separated address list. */
export function getPeerIp(p: { ip: string }): string {
  return p.ip.split(',')[0]?.trim() || ''
}

export function peerHost(p: IPeer): string {
  return `${getPeerIp(p)}:${p.port}`
}

/**
 * Remote devices with an active login token. Tauri: reactive mirror of the
 * peers-table rows (single source of truth — kept fresh by the resident
 * mDNS listener), preloaded at bootstrap so synchronous readers
 * (router guard, `getCurrentAuthToken`) never await an invoke.
 * Web: always empty — the single session token lives in localStorage.
 */
export const loginPeers = ref<LoginPeer[]>([])

export async function preloadLoginPeers(): Promise<void> {
  if (!__IS_TAURI__) return
  try {
    loginPeers.value = await invoke<LoginPeer[]>('list_login_peers')
  } catch (e) {
    console.error('list_login_peers failed', e)
  }
}

export function findLoginPeer(clientId: string): LoginPeer | undefined {
  return loginPeers.value.find((p) => p.id === clientId)
}

/** Records a successful login: creates/refreshes the peer row with the
 *  token and optional chat key. Web mode stores the
 *  token in localStorage instead. */
export async function saveLoginPeer(input: SaveLoginPeerInput): Promise<void> {
  if (!__IS_TAURI__) {
    localStorage.setItem(AUTH_TOKEN_KEY, input.token)
    return
  }
  await invoke('login_peer', {
    id: input.clientId,
    name: input.name,
    host: input.host,
    deviceType: input.deviceType,
    token: input.token,
    signaturePublicKey: input.signaturePublicKey,
    chatKey: input.chatKey,
  })
  await preloadLoginPeers()
}

/** Clears the login token (logout / forget device). The peer row itself is
 *  kept so pairing state survives. Web mode drops the localStorage token. */
export async function clearLoginPeer(clientId: string): Promise<void> {
  if (!__IS_TAURI__) {
    localStorage.removeItem(AUTH_TOKEN_KEY)
    return
  }
  await invoke('logout_peer', { id: clientId })
  await preloadLoginPeers()
}

/** Updates a peer's display name (e.g. from the device's reported name). */
export async function updateLoginPeerName(clientId: string, name: string): Promise<void> {
  if (!__IS_TAURI__ || !name) return
  const peer = findLoginPeer(clientId)
  if (!peer || peer.name === name) return
  await invoke('update_peer_name', { id: clientId, name })
  peer.name = name
}
