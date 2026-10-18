import { computed, ref } from 'vue'
import { storeToRefs } from 'pinia'
import emitter from '@/plugins/eventbus'
import type { INotification } from '@/lib/interfaces'
import { DeviceType } from '@/lib/status'
import { deleteNotificationsGQL, replyNotificationGQL } from '@/lib/api/mutation'
import { gqlFetchPeer } from '@/lib/api/peer-client'
import { proxyHttpUrl } from '@/lib/api/api'
import { findLoginPeer } from '@/lib/device/login-peers'
import { isLocalMode } from '@/lib/device/local-mode'
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
let started = false

function displayItem(item: INotification): INotification {
  return { ...item, icon: item.icon ? proxyHttpUrl(item.icon) : '' }
}

function displayGroup(group: PeerNotificationGroup): PeerNotificationGroup {
  return { ...group, items: group.items.map(displayItem) }
}

export function applyPeerNotificationUpdate(update: PeerNotificationUpdate): INotification[] {
  if (!Number.isSafeInteger(update.revision) || update.revision < 0 || update.revision <= revision) return []
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
  return update.groups ? [] : (update.alerts ?? []).map(displayItem)
}

export function startLocalPeerData() {
  if (started || !__IS_TAURI__ || !isLocalMode()) return
  started = true
  const { notificationVolume } = storeToRefs(useMainStore())
  emitter.on('peer_notifications_updated', (update) => {
    for (const item of applyPeerNotificationUpdate(update)) {
      if (notificationVolume.value > 0) playNotificationSound(notificationVolume.value)
      void showDesktopNotification({ title: item.title, body: item.body, icon: item.icon, silent: true })
    }
    if (update.permissionsChanged) emitter.emit('tap_phone', '')
  })
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
