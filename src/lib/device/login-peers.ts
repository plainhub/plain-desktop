import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
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

export function getPeerIp(p: { ip: string }): string {
  return p.ip.split(',')[0]?.trim() || ''
}

export function peerHost(p: IPeer): string {
  const ip = getPeerIp(p)
  return p.port && ip ? `${ip.includes(':') ? `[${ip}]` : ip}:${p.port}` : ''
}

export const loginPeers = ref<LoginPeer[]>([])

let refreshing: Promise<void> | undefined
let refreshPending = false

export function preloadLoginPeers(): Promise<void> {
  if (!__IS_TAURI__) return Promise.resolve()
  refreshPending = true
  if (!refreshing) refreshing = refreshLoginPeers()
  return refreshing
}

async function refreshLoginPeers(): Promise<void> {
  try {
    while (refreshPending) {
      refreshPending = false
      try {
        const result = await localData<{ peers: LoginPeer[] }>('query { peers { id name ip status port deviceType token publicKey createdAt updatedAt online address { bestIp baseUrl } } }')
        loginPeers.value = result.peers.filter((peer) => !!peer.token).sort((a, b) => b.createdAt.localeCompare(a.createdAt) || a.id.localeCompare(b.id))
      } catch (error) {
        console.error('list_login_peers failed', error)
      }
    }
  } finally {
    refreshing = undefined
  }
}

export async function subscribeLoginPeerUpdates(): Promise<() => void> {
  if (!__IS_TAURI__) return () => {}
  const stop = await listen('PEER_UPDATED', () => preloadLoginPeers())
  await preloadLoginPeers()
  return stop
}

export function findLoginPeer(clientId: string): LoginPeer | undefined {
  return loginPeers.value.find((p) => p.id === clientId)
}

export function saveBrowserSession(token: string): void {
  localStorage.setItem(AUTH_TOKEN_KEY, token)
}

export function getBrowserIdentity(): { deviceId: string; publicKey: string } | undefined {
  const saved = localStorage.getItem('login_identity')
  if (!saved) return
  try {
    const identity = JSON.parse(saved)
    if (typeof identity.deviceId === 'string' && identity.deviceId && typeof identity.publicKey === 'string' && identity.publicKey) return identity
  } catch {
    throw 'peer_identity_changed'
  }
  throw 'peer_identity_changed'
}

export function saveBrowserIdentity(deviceId: string, publicKey: string): void {
  const saved = getBrowserIdentity()
  if (saved && (saved.deviceId !== deviceId || saved.publicKey !== publicKey)) throw 'peer_identity_changed'
  localStorage.setItem('login_identity', JSON.stringify({ deviceId, publicKey }))
}

export function forgetBrowserIdentity(): void {
  localStorage.removeItem('login_identity')
  localStorage.removeItem(AUTH_TOKEN_KEY)
}

/** Clears the login token (logout / forget device). The peer row itself is
 *  kept so pairing state survives. Web mode drops the localStorage token. */
export async function clearLoginPeer(clientId: string): Promise<void> {
  if (!__IS_TAURI__) {
    localStorage.removeItem(AUTH_TOKEN_KEY)
    return
  }
  await invoke('native_logout_peer', { id: clientId })
  await preloadLoginPeers()
}
