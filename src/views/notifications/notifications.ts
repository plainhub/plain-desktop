import { onMounted, ref } from 'vue'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { initQuery, notificationsGQL } from '@/lib/api/query'
import { initMutation, cancelNotificationsGQL, replyNotificationGQL } from '@/lib/api/mutation'
import type { INotification } from '@/lib/interfaces'
import { getFileUrlByPath } from '@/lib/api/file'
import emitter from '@/plugins/eventbus'
import { useNotificationWarning } from '@/hooks/notification-warning'
import { playNotificationSound } from '@/lib/notification-sound'
import { showDesktopNotification } from '@/lib/desktop-notification'

export function useNotifications() {
  const store = useMainStore()
  const { notificationVolume } = storeToRefs(store)
  const { app, urlTokenKey } = storeToRefs(useTempStore())
  const { hasWarning: hasNotificationWarning, warningMessage: notificationWarningMessage, warningAction: notificationWarningAction } = useNotificationWarning()

  const notifications = ref<INotification[]>([])

  const { loading, refetch } = initQuery({
    handle: (data: any, error: string) => {
      if (data) {
        notifications.value = data.notifications.map((it: any) => ({ ...it, icon: getFileUrlByPath(urlTokenKey.value, 'pkgicon://' + it.appId) }))
      }
    },
    document: notificationsGQL,
  })

  const { mutate: cancelNotifications } = initMutation({ document: cancelNotificationsGQL })
  const { mutate: replyNotification, loading: replySending, onDone: onReplyDone, onError: onReplyError } = initMutation({ document: replyNotificationGQL })

  // Reply state
  const replyingId = ref<string | null>(null)
  const replyingActionIndex = ref<number>(0)

  function startReply(id: string, actionIndex: number) { replyingId.value = id; replyingActionIndex.value = actionIndex }
  function cancelReply() { replyingId.value = null }
  function sendReply(id: string, text: string) {
    if (!text) return
    replyNotification({ id, actionIndex: replyingActionIndex.value, text })
  }

  onReplyDone(() => cancelReply())
  onReplyError(() => cancelReply())

  const deleteItem = (item: INotification) => cancelNotifications({ ids: [item.id] })
  const clearAll = () => {
    const ids = notifications.value.map((it) => it.id)
    notifications.value = []
    cancelNotifications({ ids })
  }

  // Desktop notification + sound helper
  function notify(data: INotification) {
    if (notificationVolume.value > 0) playNotificationSound(notificationVolume.value)
    showDesktopNotification({ title: data.title, body: data.body, icon: data.icon, silent: true })
  }

  onMounted(() => {
    emitter.on('notification_created', async (data: any) => {
      data.icon = getFileUrlByPath(urlTokenKey.value, 'pkgicon://' + data.appId)
      notifications.value = [{ ...data }, ...notifications.value]
      notify(data)
    })

    emitter.on('notification_updated', async (data: any) => {
      data.icon = getFileUrlByPath(urlTokenKey.value, 'pkgicon://' + data.appId)
      notifications.value = notifications.value.map((n) => n.id === data.id ? { ...data } : n)
      notify(data)
    })

    emitter.on('notification_deleted', async (data: any) => {
      notifications.value = notifications.value.filter((n) => n.id !== data.id)
    })

    emitter.on('notification_refreshed', async () => refetch())
  })

  return {
    store, app, notificationVolume, notifications, loading,
    hasNotificationWarning, notificationWarningMessage, notificationWarningAction,
    replyingId, replySending,
    startReply, cancelReply, sendReply, deleteItem, clearAll,
  }
}
