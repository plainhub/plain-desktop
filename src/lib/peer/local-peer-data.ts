import { isLocalMode } from '@/lib/device/local-mode'
import { computed, ref } from 'vue'
import { Channel, invoke } from '@tauri-apps/api/core'
import { storeToRefs } from 'pinia'
import emitter from '@/plugins/eventbus'
import type { INotification } from '@/lib/interfaces'
import { DeviceType } from '@/lib/status'
import { deleteNotificationsGQL, replyNotificationGQL } from '@/lib/api/mutation'
import { gqlFetchPeer } from '@/lib/api/peer-client'
import { proxyHttpUrl } from '@/lib/api/api'
import { findLoginPeer } from '@/lib/device/login-peers'
import { playNotificationSound } from '@/lib/notification-sound'
import { showDesktopNotification } from '@/lib/desktop-notification'
import { useMainStore } from '@/stores/main'

export interface PeerNotificationGroup {
  peerId: string
  name: string
  deviceType: DeviceType
  online: boolean
  loading: boolean
  loaded: boolean
  capabilities: string[] | null
  permissions: string[]
  items: INotification[]
}

export type PeerNotificationUpdate = {
  revision: number
  groups?: PeerNotificationGroup[]
  peerId?: string
  group?: PeerNotificationGroup | null
  alerts?: INotification[]
  permissionsChanged?: boolean
}

const groups = ref<PeerNotificationGroup[]>([])
export const peerNotificationGroups = computed(() => groups.value.filter((group) => group.capabilities === null || group.capabilities.includes('NOTIFICATIONS')))
let revision = -1
let alertsRevision = -1
let started = false

function displayItem(item: INotification): INotification {
  return { ...item, icon: item.icon ? proxyHttpUrl(item.icon) : '' }
}

function displayGroup(group: PeerNotificationGroup): PeerNotificationGroup {
  return { ...group, items: group.items.map(displayItem) }
}

function takeAlerts(update: PeerNotificationUpdate): INotification[] {
  if (!update.alerts?.length || update.revision <= alertsRevision) return []
  alertsRevision = update.revision
  return update.alerts.map(displayItem)
}

export function applyPeerNotificationUpdate(update: PeerNotificationUpdate): INotification[] {
  if (!Number.isSafeInteger(update.revision) || update.revision < 0 || update.revision < revision) return []
  if (update.revision === revision) {
    if (!update.groups) return []
    groups.value = update.groups.map(displayGroup)
    return takeAlerts(update)
  }
  if (update.groups) {
    groups.value = update.groups.map(displayGroup)
  } else if (typeof update.peerId === 'string' && (update.group === null || update.group?.peerId === update.peerId)) {
    const index = groups.value.findIndex((group) => group.peerId === update.peerId)
    if (update.group) {
      const group = displayGroup(update.group)
      if (index < 0) groups.value.push(group)
      else groups.value[index] = group
    } else if (index >= 0) {
      groups.value.splice(index, 1)
    }
  } else {
    return []
  }
  revision = update.revision
  return takeAlerts(update)
}

export function startLocalPeerData() {
  if (started || !isLocalMode()) return
  started = true
  const { notificationVolume } = storeToRefs(useMainStore())
  function showAlerts(items: INotification[]) {
    for (const item of items) {
      if (notificationVolume.value > 0) playNotificationSound(notificationVolume.value)
      void showDesktopNotification({ title: item.title, body: item.body, icon: item.icon, silent: true })
    }
  }
  function apply(update: PeerNotificationUpdate) {
    showAlerts(applyPeerNotificationUpdate(update))
    if (update.permissionsChanged) emitter.emit('tap_phone', '')
  }
  void (async () => {
    let initialized = false
    const buffered: PeerNotificationUpdate[] = []
    const channel = new Channel<PeerNotificationUpdate>()
    channel.onmessage = (payload) => {
      if (initialized) apply(payload)
      else buffered.push(payload)
    }
    const id = await invoke<number>('native_register_peer_notifications', { channel })
    window.addEventListener('beforeunload', () => {
      void invoke('native_unregister_peer_notifications', { id }).catch(console.error)
    }, { once: true })
    const snapshot = await invoke<PeerNotificationUpdate>('native_peer_notifications')
    apply(snapshot)
    for (const update of buffered) {
      if (update.revision <= snapshot.revision) {
        showAlerts(takeAlerts(update))
        if (update.permissionsChanged) emitter.emit('tap_phone', '')
      } else apply(update)
    }
    initialized = true
  })().catch((error) => console.error('Peer notifications initialization failed', error))
}

export function dropPeerNotifications(peerId: string, ids: string[]) {
  const peer = findLoginPeer(peerId)
  if (peer && ids.length) void gqlFetchPeer(peer, deleteNotificationsGQL, { ids }).catch(() => {})
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
