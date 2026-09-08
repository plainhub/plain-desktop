import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import {
  peerNotificationGroups,
  dropPeerNotifications,
  openPeerNotificationSettings,
  replyPeerNotification,
} from '@/lib/peer/local-peer-data'

export type { PeerNotificationGroup } from '@/lib/peer/local-peer-data'

/** UI binding over the resident local-peer data layer: transient reply state
 *  and offline-aware action policies live here, data lives in lib/peer. */
export function useLocalNotifications() {
  const { t } = useI18n()

  const groups = peerNotificationGroups
  const total = computed(() => groups.value.reduce((n, g) => n + g.items.length, 0))

  const replyingId = ref('')
  const replyingActionIndex = ref(0)
  const replySending = ref(false)

  const startReply = (peerId: string, id: string, actionIndex: number) => {
    replyingId.value = `${peerId}:${id}`
    replyingActionIndex.value = actionIndex
  }
  const cancelReply = () => {
    replyingId.value = ''
  }

  async function sendReply(peerId: string, id: string, text: string) {
    if (!text) return
    replySending.value = true
    try {
      if (await replyPeerNotification(peerId, id, replyingActionIndex.value, text)) replyingId.value = ''
    } finally {
      replySending.value = false
    }
  }

  const deleteItem = (peerId: string, id: string) => dropPeerNotifications(peerId, [id])

  function clearGroup(peerId: string) {
    const group = groups.value.find((g) => g.peerId === peerId)
    if (group) dropPeerNotifications(peerId, group.items.map((it) => it.id))
  }

  function clearAll() {
    for (const g of groups.value) {
      if (g.online) clearGroup(g.peerId)
    }
  }

  async function openSettings(peerId: string) {
    await openPeerNotificationSettings(peerId)
    toast(t('check_phone'))
  }

  return {
    groups, total,
    replyingId, replySending,
    startReply, cancelReply, sendReply,
    deleteItem, clearGroup, clearAll, openSettings,
  }
}
