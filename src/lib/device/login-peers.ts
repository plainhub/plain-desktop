import { ref } from 'vue'
import { nativeGraphql } from '@/lib/api/native-client'
import type { IPeer } from '@/lib/interfaces'

const AUTH_TOKEN_KEY = 'auth_token'

async function localData<T>(query: string, variables?: Record<string, unknown>): Promise<T> {
  const result = await nativeGraphql<{ data: T; errors?: { message: string }[] }>({ query, variables }, 30000)
  if (result.errors?.length) throw new Error(result.errors[0].message)
  return result.data
}

export type LoginPeer = IPeer & { token: string; publicKey: string; address?: { bestIp: string; baseUrl: string } }

/** First IP from a comma-separated address list. */
export function getPeerIp(p: { ip: string }): string {
  return p.ip.split(',')[0]?.trim() || ''
}

export function peerHost(p: IPeer): string {
  const ip = getPeerIp(p)
  return p.port && ip ? `${ip.includes(':') ? `[${ip}]` : ip}:${p.port}` : ''
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
    const result = await localData<{ peers: LoginPeer[] }>(
      'query { peers { id name ip status port deviceType token publicKey createdAt updatedAt online address { bestIp baseUrl } } }',
    )
    loginPeers.value = result.peers.filter((peer) => !!peer.token).sort((a, b) => b.createdAt.localeCompare(a.createdAt) || a.id.localeCompare(b.id))
  } catch (e) {
    console.error('list_login_peers failed', e)
  }
}

export function findLoginPeer(clientId: string): LoginPeer | undefined {
  return loginPeers.value.find((p) => p.id === clientId)
}

export function saveBrowserSession(token: string): void {
  localStorage.setItem(AUTH_TOKEN_KEY, token)
}

/** Clears the login token (logout / forget device). The peer row itself is
 *  kept so pairing state survives. Web mode drops the localStorage token. */
export async function clearLoginPeer(clientId: string): Promise<void> {
  if (!__IS_TAURI__) {
    localStorage.removeItem(AUTH_TOKEN_KEY)
    return
  }
  await localData('mutation($id: ID!) { logoutPeer(id: $id) }', { id: clientId })
  await preloadLoginPeers()
}

/** Updates a peer's display name (e.g. from the device's reported name). */
export async function updateLoginPeerName(clientId: string, name: string): Promise<void> {
  if (!__IS_TAURI__ || !name) return
  const peer = findLoginPeer(clientId)
  if (!peer || peer.name === name) return
  await localData('mutation($id: ID!, $name: String!) { updatePeerName(id: $id, name: $name) }', { id: clientId, name })
  peer.name = name
}
