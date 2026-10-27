import { ref, watch } from 'vue'
import { Channel, invoke } from '@tauri-apps/api/core'
import type { IClipboardItem } from '@/lib/interfaces'
import { DeviceType } from '@/lib/status'
import { clipboardItemFragment } from '@/lib/api/fragments'
import { deleteClipboardItemsGQL } from '@/lib/api/mutation'
import { gqlFetchPeer } from '@/lib/api/peer-client'
import { findLoginPeer, loginPeers } from '@/lib/device/login-peers'
import { isLocalMode } from '@/lib/device/local-mode'
import { get as prefsGet, set as prefsSet } from '@/lib/prefs'
import { useMainStore } from '@/stores/main'

const DIRECTIONS_PREF_KEY = 'clipboard_directions'

export type ClipboardDirection = 'off' | 'push' | 'pull' | 'both'

function readDirections(): Record<string, ClipboardDirection> {
  return prefsGet<Record<string, ClipboardDirection>>(DIRECTIONS_PREF_KEY, {})
}

export const clipboardDirections = ref<Record<string, ClipboardDirection>>(readDirections())

export function peerClipboardDirection(peerId: string): ClipboardDirection {
  return clipboardDirections.value[peerId] ?? 'pull'
}

const PEER_CLIPBOARD_GQL = `
  query clipboardItems($offset: Int!, $limit: Int!, $query: String!) {
    clipboardItems(query: $query, offset: $offset, limit: $limit) {
      ...ClipboardItemFragment
    }
    clipboardItemCount(query: $query)
  }
  ${clipboardItemFragment}
`

export interface PeerClipboardGroup {
  peerId: string
  name: string
  deviceType: DeviceType
  /** Reflects the last data fetch only — never WS lifecycle, so it never flaps. */
  online: boolean
  /** True only until the first fetch settles; background refreshes never show it. */
  loading: boolean
  loaded: boolean
  /** Phone-side master switch inferred from the list fetch: true = served,
   *  false = peer answered clipboard_sync_disabled, null = not attempted yet. */
  clipboardSync: boolean | null
  page: number
  total: number
  items: IClipboardItem[]
}

/** Resident aggregation state — lives for the whole app session in local mode,
 *  independent of any panel. */
export const peerClipboardGroups = ref<PeerClipboardGroup[]>([])

let started = false

function groupOf(peerId: string): PeerClipboardGroup | undefined {
  return peerClipboardGroups.value.find((g) => g.peerId === peerId)
}

function limit() {
  return useMainStore().pageSize
}

async function fetchPeerClipboard(peerId: string, silent = false) {
  const peer = findLoginPeer(peerId)
  const group = groupOf(peerId)
  if (!peer || !group) return
  if (group.clipboardSync === false) return
  if (!silent && !group.loaded) group.loading = true
  try {
    const res = await gqlFetchPeer<{ clipboardItems: IClipboardItem[]; clipboardItemCount: number }>(
      peer,
      PEER_CLIPBOARD_GQL,
      { offset: (group.page - 1) * limit(), limit: limit(), query: '' },
    )
    if (res.errors?.length) {
      if (res.errors[0].message === 'clipboard_sync_disabled') group.clipboardSync = false
    } else {
      group.clipboardSync = true
      group.items = res.data?.clipboardItems ?? []
      group.total = res.data?.clipboardItemCount ?? 0
    }
    group.online = true
  } catch {
    group.online = false
  } finally {
    group.loaded = true
    group.loading = false
  }
}

function syncGroups() {
  const alive = new Set(loginPeers.value.map((p) => p.id))
  for (const g of [...peerClipboardGroups.value]) {
    if (!alive.has(g.peerId)) {
      peerClipboardGroups.value = peerClipboardGroups.value.filter((it) => it.peerId !== g.peerId)
    }
  }
  for (const p of loginPeers.value) {
    const group = groupOf(p.id)
    if (group) {
      group.name = p.name
      group.deviceType = p.deviceType
      continue
    }
    peerClipboardGroups.value.push({
      peerId: p.id, name: p.name, deviceType: p.deviceType,
      online: true, loading: true, loaded: false, clipboardSync: null, page: 1, total: 0, items: [],
    })
    fetchPeerClipboard(p.id)
  }
}

function refreshChangedPeerClipboard(peerId: string) {
  if (!isLocalMode()) return
  const group = groupOf(peerId)
  if (!group || !findLoginPeer(peerId)) return
  if (group.clipboardSync === false) group.clipboardSync = null
  fetchPeerClipboard(peerId, true)
}

export function setPeerClipboardDirection(peerId: string, direction: ClipboardDirection) {
  clipboardDirections.value = { ...clipboardDirections.value, [peerId]: direction }
  prefsSet(DIRECTIONS_PREF_KEY, clipboardDirections.value)
  const group = groupOf(peerId)
  if (!group) return
  group.page = 1
  group.clipboardSync = null
  fetchPeerClipboard(peerId)
}

/** Idempotent bootstrap — call once from the app root in local mode. */
export function startLocalClipboardData() {
  if (started || !__IS_TAURI__ || !isLocalMode()) return
  started = true

  void (async () => {
    const channel = new Channel<{ peerIds: string[]; refreshAll: boolean }>()
    channel.onmessage = ({ peerIds, refreshAll }) => {
      const ids = refreshAll ? peerClipboardGroups.value.map((group) => group.peerId) : peerIds
      for (const id of ids) refreshChangedPeerClipboard(id)
    }
    const id = await invoke<number>('native_register_peer_clipboard', { channel })
    window.addEventListener('beforeunload', () => {
      void invoke('native_unregister_peer_clipboard', { id }).catch(console.error)
    }, { once: true })
    for (const group of peerClipboardGroups.value) refreshChangedPeerClipboard(group.peerId)
  })().catch((error) => console.error('Peer clipboard initialization failed', error))

  clipboardDirections.value = readDirections()
  syncGroups()
  watch(loginPeers, syncGroups)
}

export function gotoPeerClipboardPage(peerId: string, page: number) {
  const group = groupOf(peerId)
  if (!group) return
  group.page = page
  fetchPeerClipboard(peerId)
}

/** Optimistically drops clipboard entries locally and cancels them on the peer. */
export function dropPeerClipboard(peerId: string, ids: string[]) {
  const group = groupOf(peerId)
  if (!group || !ids.length) return
  const idSet = new Set(ids)
  group.items = group.items.filter((it) => !idSet.has(it.id))
  group.total = Math.max(0, group.total - ids.length)
  const peer = findLoginPeer(peerId)
  if (peer) void gqlFetchPeer(peer, deleteClipboardItemsGQL, { query: `ids:${ids.join(',')}` }).catch(() => {})
}
