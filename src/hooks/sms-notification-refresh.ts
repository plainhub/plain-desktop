import { graphqlSubscriptions } from '@/lib/api/subscription-client'

export const SMS_NOTIFICATION_REFRESH_DELAY_MS = 500
const MAX_REFRESH_DELAY_MS = 2000
const SAFETY_REFRESH_INTERVAL_MS = 60_000

export function createSmsNotificationRefresh(refresh: () => void | Promise<unknown>, reconnect?: () => void) {
  let timeout: ReturnType<typeof setTimeout> | undefined
  let maximumTimeout: ReturnType<typeof setTimeout> | undefined
  let safetyTimer: ReturnType<typeof setInterval> | undefined
  let subscribed = false
  let refreshing = false
  let refreshQueued = false

  let stopMmsSent: (() => void) | undefined
  let stopSmsProviderChanged: (() => void) | undefined
  let stopAttached: (() => void) | undefined

  function finishRefresh() {
    refreshing = false
    if (subscribed && refreshQueued) {
      refreshQueued = false
      handleSmsChanged()
    }
  }

  function refreshOnce() {
    if (!subscribed) return
    if (refreshing) {
      refreshQueued = true
      return
    }
    refreshing = true
    try {
      const result = refresh()
      if (result) {
        void result.then(finishRefresh, () => {
          console.error('SMS refresh failed')
          finishRefresh()
        })
      } else {
        finishRefresh()
      }
    } catch (error) {
      finishRefresh()
      throw error
    }
  }

  function clearScheduledRefresh() {
    if (timeout) clearTimeout(timeout)
    if (maximumTimeout) clearTimeout(maximumTimeout)
    timeout = undefined
    maximumTimeout = undefined
  }

  function flush() {
    clearScheduledRefresh()
    refreshOnce()
  }

  function handleSmsChanged() {
    if (timeout) clearTimeout(timeout)
    timeout = setTimeout(flush, SMS_NOTIFICATION_REFRESH_DELAY_MS)
    maximumTimeout ??= setTimeout(flush, MAX_REFRESH_DELAY_MS)
  }

  function handleResume() {
    if (document.visibilityState === 'visible') handleSmsChanged()
  }

  function handleConnectionChanged() {
    refreshOnce()
    reconnect?.()
  }

  function handleMmsSent() {
    handleSmsChanged()
  }

  function subscribe() {
    if (subscribed) return
    subscribed = true
    stopSmsProviderChanged = graphqlSubscriptions.subscribe('smsProviderChanged', handleSmsChanged)
    stopMmsSent = graphqlSubscriptions.subscribe('mmsSent', handleMmsSent)
    stopAttached = graphqlSubscriptions.onAttached(handleConnectionChanged)
    document.addEventListener('visibilitychange', handleResume)
    window.addEventListener('focus', handleResume)
    window.addEventListener('online', handleResume)
    safetyTimer = setInterval(() => {
      if (document.visibilityState === 'visible' && navigator.onLine) refreshOnce()
    }, SAFETY_REFRESH_INTERVAL_MS)
  }

  function unsubscribe() {
    if (!subscribed) return
    subscribed = false
    refreshQueued = false
    stopSmsProviderChanged?.()
    stopMmsSent?.()
    stopAttached?.()
    document.removeEventListener('visibilitychange', handleResume)
    window.removeEventListener('focus', handleResume)
    window.removeEventListener('online', handleResume)
    if (safetyTimer) clearInterval(safetyTimer)
    safetyTimer = undefined
    clearScheduledRefresh()
  }

  return { subscribe, unsubscribe }
}
