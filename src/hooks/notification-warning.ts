import { ref, computed } from 'vue'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { pushModal } from '@/components/modal'
import ConfirmModal from '@/components/ConfirmModal.vue'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import { buildUrl } from '@/lib/url'

type PermissionState = 'granted' | 'denied' | 'default'

const notificationPermission = ref<PermissionState>(
  __IS_TAURI__
    ? 'default'
    : ('Notification' in window && typeof Notification !== 'undefined'
        ? (Notification.permission as PermissionState)
        : 'default')
)

// In Tauri mode, check native permission state asynchronously on module load.
if (__IS_TAURI__) {
  import('@/lib/tauri-plugins').then(({ isPermissionGranted }) => {
    isPermissionGranted().then((granted) => {
      // isPermissionGranted returns false both for 'default' and 'denied'.
      // We only update to 'granted' here; 'denied' is set after an explicit request.
      if (granted) notificationPermission.value = 'granted'
    }).catch(() => {})
  })
}

const isHttps = !__IS_TAURI__ && window.location.protocol === 'https:'

export function useNotificationWarning(options?: { showToast?: boolean }) {
  const { app } = storeToRefs(useTempStore())
  const { t } = useI18n()

  const hasWarning = computed(() => notificationPermission.value !== 'granted')

  const warningMessage = computed(() => {
    if (__IS_TAURI__) {
      return notificationPermission.value === 'denied'
        ? 'desktop_notification_permission_denied'
        : 'desktop_notification_permission_not_granted'
    }
    if (!isHttps && notificationPermission.value !== 'granted') {
      return 'desktop_notification_need_https'
    }
    return notificationPermission.value !== 'granted'
      ? 'desktop_notification_permission_not_granted'
      : ''
  })

  const useHttpsLink = () => {
    window.open(buildUrl('https', window.location.hostname, app.value.httpsPort), '_blank')
  }

  // Open macOS System Settings > Notifications.
  const openSystemNotificationSettings = () => {
    import('@/lib/tauri-plugins').then(({ openUrl }) => {
      openUrl('x-apple.systempreferences:com.apple.preference.notifications')
        .catch(() => openUrl('x-apple.systempreferences:'))
    })
  }

  const grantPermission = () => {
    if (__IS_TAURI__) {
      if (notificationPermission.value === 'denied') {
        // Cannot re-request once denied; direct the user to system settings.
        openSystemNotificationSettings()
        return
      }
      import('@/lib/tauri-plugins').then(({ requestPermission }) => {
        requestPermission().then((result) => {
          notificationPermission.value = result as PermissionState
          if (result === 'granted' && options?.showToast) {
            toast(t('desktop_notification_permission_granted'))
          }
        }).catch(() => {})
      })
      return
    }

    if (!('Notification' in window) || typeof Notification === 'undefined') return

    if (Notification.permission === 'denied') {
      if (options?.showToast) {
        toast(t('desktop_notification_permission_denied_help'), 'error')
      } else {
        pushModal(ConfirmModal, {
          title: t('desktop_notification_permission_grant_title'),
          message: t('desktop_notification_permission_grant_message'),
        })
      }
      return
    }

    Notification.requestPermission().then((permission) => {
      notificationPermission.value = permission as PermissionState
      if (options?.showToast && permission === 'granted') {
        toast(t('desktop_notification_permission_granted'))
      }
    })
  }

  const warningAction = computed(() => {
    if (notificationPermission.value === 'granted') return null

    if (__IS_TAURI__) {
      return notificationPermission.value === 'denied'
        ? { text: 'notification_open_settings', action: openSystemNotificationSettings }
        : { text: 'grant_permission', action: grantPermission }
    }

    if (!isHttps) return { text: 'use_https_link', action: useHttpsLink }
    return { text: 'grant_permission', action: grantPermission }
  })

  return {
    hasWarning,
    warningMessage,
    warningAction,
    notificationPermission,
    useHttpsLink,
    grantPermission,
    isHttps,
  }
}
