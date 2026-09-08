import { ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import emitter from '@/plugins/eventbus'
import type { INotification } from '@/lib/interfaces'
import { DeviceType } from '@/lib/status'
import { notificationFragment } from '@/lib/api/fragments'
import { cancelNotificationsGQL, replyNotificationGQL, openWebSettingsGQL } from '@/lib/api/mutation'
import { gqlFetchPeer } from '@/lib/api/peer-client'
import { deviceBaseUrl, proxyUrlFor } from '@/lib/api/api'
import { chachaEncrypt, bitArrayToBase64 } from '@/lib/api/crypto'
import { tokenToKey } from '@/lib/api/file'
import { loginPeers, peerHost, findLoginPeer } from '@/lib/device/login-peers'
import { isLocalMode } from '@/lib/device/local-mode'
import { playNotificationSound } from '@/lib/notification-sound'
import { showDesktopNotification } from '@/lib/desktop-notification'
import { removeNotification, upsertNotification } from '@/lib/notification-groups'
import { startPeerSockets } from './peer-sockets'
import { useMainStore } from '@/stores/main'

const PEER_NOTIFICATIONS_GQL = `
  query {
    notifications {
      ...NotificationFragment
    }
    app {
      permissions
      urlToken
    }
  }
  ${notificationFragment}
`

const NOTIFICATION_EVENTS = new Set([7, 8, 9, 10])
const REFRESH_INTERVAL = 60000

export interface PeerNotificationGroup {
  peerId: string
  name: string
  deviceType: DeviceType
  /** Reflects the last data fetch only — never WS lifecycle, so it never flaps. */
  online: boolean
  /** True only until the first fetch settles; background refreshes never show it. */
  loading: boolean
  loaded: boolean
  /** Last known permissions reported by the peer's own `app` query. */
  permissions: string[]
  /** Peer's urlToken — `/fs` file ids (pkgicon) are encrypted with this, not the login token. */
  urlToken: string
  items: INotification[]
}

/** Resident aggregation state — lives for the whole app session in local mode,
 *  independent of any panel. Clipboard aggregation will live beside it. */
export const peerNotificationGroups = ref<PeerNotificationGroup[]>([])

const iconCache = new Map<string, { urlToken: string; icon: string }>()
let started = false

function groupOf(peerId: string): PeerNotificationGroup | undefined {
  return peerNotificationGroups.value.find((g) => g.peerId === peerId)
}

function decorate(group: PeerNotificationGroup, n: INotification): INotification {
  const peer = findLoginPeer(group.peerId)
  if (!peer || !group.urlToken) return { ...n, icon: '' }
  const cacheKey = `${group.peerId}:${n.appId}`
  const hit = iconCache.get(cacheKey)
  let icon = hit && hit.urlToken === group.urlToken ? hit.icon : ''
  if (!icon) {
    const id = bitArrayToBase64(chachaEncrypt(tokenToKey(group.urlToken), 'pkgicon://' + n.appId))
    icon = proxyUrlFor(deviceBaseUrl(peerHost(peer)), `/fs?id=${encodeURIComponent(id)}`)
    iconCache.set(cacheKey, { urlToken: group.urlToken, icon })
  }
  return { ...n, icon }
}

function fetchPeerNotifications(peerId: string, silent = false) {
  const peer = findLoginPeer(peerId)
  const group = groupOf(peerId)
  if (!peer || !group) return
  if (!silent && !group.loaded) group.loading = true
  gqlFetchPeer<{ notifications: INotification[]; app: { permissions: string[]; urlToken: string } }>(
    peer,
    PEER_NOTIFICATIONS_GQL,
  )
    .then((res) => {
      group.urlToken = res.data?.app?.urlToken ?? group.urlToken
      group.items = (res.data?.notifications ?? []).map((n) => decorate(group, n))
      group.permissions = res.data?.app?.permissions ?? []
      group.online = true
    })
    .catch(() => {
      group.online = false
    })
    .finally(() => {
      group.loaded = true
      group.loading = false
    })
}

function syncGroups() {
  const alive = new Set(loginPeers.value.map((p) => p.id))
  for (const g of [...peerNotificationGroups.value]) {
    if (!alive.has(g.peerId)) {
      peerNotificationGroups.value = peerNotificationGroups.value.filter((it) => it.peerId !== g.peerId)
    }
  }
  for (const p of loginPeers.value) {
    const group = groupOf(p.id)
    if (group) {
      group.name = p.name
      group.deviceType = p.deviceType
      continue
    }
    peerNotificationGroups.value.push({
      peerId: p.id, name: p.name, deviceType: p.deviceType,
      online: true, loading: true, loaded: false, permissions: [], urlToken: '', items: [],
    })
    fetchPeerNotifications(p.id)
  }
}

/** Idempotent bootstrap — call once from the app root in local mode. */
export function startLocalPeerData() {
  if (started || !__IS_TAURI__ || !isLocalMode()) return
  started = true
  const { notificationVolume } = storeToRefs(useMainStore())

  emitter.on('peer_ws_event', ({ peerId, type, data }) => {
    if (!NOTIFICATION_EVENTS.has(type)) return
    const group = groupOf(peerId)
    if (!group || !findLoginPeer(peerId)) return
    const decorated = decorate(group, data)
    if (type === 7) {
      group.items = upsertNotification(group.items, decorated)
      if (notificationVolume.value > 0) playNotificationSound(notificationVolume.value)
      showDesktopNotification({ title: decorated.title, body: decorated.body, icon: decorated.icon, silent: true })
    } else if (type === 8) {
      group.items = upsertNotification(group.items, decorated)
    } else if (type === 9) {
      group.items = removeNotification(group.items, data.id)
    } else {
      fetchPeerNotifications(peerId, true)
    }
  })

  syncGroups()
  watch(loginPeers, syncGroups)
  startPeerSockets()
  setInterval(() => {
    for (const g of peerNotificationGroups.value) fetchPeerNotifications(g.peerId, true)
  }, REFRESH_INTERVAL)
}

/** Optimistically drops notifications locally and cancels them on the peer. */
export function dropPeerNotifications(peerId: string, ids: string[]) {
  const group = groupOf(peerId)
  if (!group || !ids.length) return
  const idSet = new Set(ids)
  group.items = group.items.filter((it) => !idSet.has(it.id))
  const peer = findLoginPeer(peerId)
  if (peer) void gqlFetchPeer(peer, cancelNotificationsGQL, { ids }).catch(() => {})
}

export async function replyPeerNotification(peerId: string, id: string, actionIndex: number, text: string): Promise<boolean> {
  const peer = findLoginPeer(peerId)
  if (!peer || !text) return false
  try {
    await gqlFetchPeer(peer, replyNotificationGQL, { id, actionIndex, text })
    return true
  } catch {
    return false
  }
}

export async function openPeerNotificationSettings(peerId: string) {
  const peer = findLoginPeer(peerId)
  if (!peer) return
  await gqlFetchPeer(peer, openWebSettingsGQL).catch(() => {})
}
