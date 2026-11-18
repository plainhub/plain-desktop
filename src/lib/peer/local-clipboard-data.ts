import { computed } from 'vue'
import { loginPeers } from '@/lib/device/login-peers'
import { PeerStatus } from '@/lib/status'
import { Capability } from '@/lib/data'
import { managedPeerCapabilities } from './local-peer-data'
import { isLocalMode } from '@/lib/device/local-mode'
import { Channel, invoke } from '@/lib/api/native-invoke'
import { get as prefsGet, set as prefsSet } from '@/lib/prefs/core'

const DIRECTIONS_PREF_KEY = 'clipboard_directions'
export const clipboardPeers = computed(() => loginPeers.value.filter(peer =>
  peer.status === PeerStatus.PAIRED && managedPeerCapabilities.value.get(peer.id)?.includes(Capability.CLIPBOARD),
))
export const findClipboardPeer = (peerId: string) => clipboardPeers.value.find(peer => peer.id === peerId)

export const clipboardDirections = ['OFF', 'SEND', 'RECEIVE', 'BOTH'] as const
export type ClipboardDirection = typeof clipboardDirections[number]
export const isClipboardDirection = (value: unknown): value is ClipboardDirection =>
  clipboardDirections.some(direction => direction === value)

function storedDirections(): Record<string, unknown> | undefined {
  const value = prefsGet<unknown>(DIRECTIONS_PREF_KEY, {})
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : undefined
}

export function peerClipboardDirection(peerId: string): ClipboardDirection {
  const values = storedDirections()
  if (!values) return 'OFF'
  if (!Object.hasOwn(values, peerId)) return 'RECEIVE'
  const value = values[peerId]
  return isClipboardDirection(value) ? value : 'OFF'
}

let directionWrites = Promise.resolve()
export function setPeerClipboardDirection(peerId: string, direction: ClipboardDirection): Promise<void> {
  const save = directionWrites.then(() => {
    if (!isLocalMode() || !findClipboardPeer(peerId)) throw new Error('Clipboard device is no longer managed')
    const values = storedDirections()
    if (!values) throw new Error('Invalid clipboard direction preferences')
    return prefsSet(DIRECTIONS_PREF_KEY, {
      ...values,
      [peerId]: direction,
    })
  })
  directionWrites = save.catch(() => {})
  return save
}

type ClipboardNotice = { local: true } | { peerIds: string[]; refreshAll: boolean }
const listeners = new Set<(notice: ClipboardNotice) => void>()
let started = false

export function startLocalClipboardData() {
  if (started || !isLocalMode()) return
  started = true
  const channel = new Channel<ClipboardNotice>()
  channel.onmessage = notice => listeners.forEach(listener => listener(notice))
  void invoke<number>('native_register_peer_clipboard', { channel }).then(id => {
    window.addEventListener('beforeunload', () => {
      void invoke('native_unregister_peer_clipboard', { id }).catch(console.error)
    }, { once: true })
    channel.onmessage({ local: true })
    channel.onmessage({ peerIds: [], refreshAll: true })
  }).catch(error => console.error('Peer clipboard initialization failed', error))
}

export function onClipboardHistoryChanged(peerId: string, refresh: () => void): () => void {
  const listener = (notice: ClipboardNotice) => {
    if ('local' in notice ? !peerId : peerId && (notice.refreshAll || notice.peerIds.includes(peerId))) refresh()
  }
  listeners.add(listener)
  return () => { listeners.delete(listener) }
}
