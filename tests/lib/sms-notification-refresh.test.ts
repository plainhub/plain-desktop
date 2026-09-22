import { afterEach, describe, expect, it, vi } from 'vitest'
import emitter from '@/plugins/eventbus'
import type { INotification } from '@/lib/interfaces'
import {
  createSmsNotificationRefresh,
  SMS_NOTIFICATION_REFRESH_DELAY_MS,
} from '@/hooks/sms-notification-refresh'

function notification(appId: string): INotification {
  return { appId } as INotification
}

const cleanups: Array<() => void> = []

afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  vi.useRealTimers()
  vi.restoreAllMocks()
})

function subscribe(refresh: () => void | Promise<unknown>) {
  const subscription = createSmsNotificationRefresh(refresh)
  subscription.subscribe()
  cleanups.push(subscription.unsubscribe)
  return subscription
}

describe('SMS notification refresh', () => {
  it('repairs a missed event with a visible-view safety refresh and stops on unsubscribe', () => {
    vi.useFakeTimers()
    const visibility = vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible')
    const refresh = vi.fn()
    const subscription = subscribe(refresh)
    vi.advanceTimersByTime(60_000)
    expect(refresh).toHaveBeenCalledOnce()
    visibility.mockReturnValue('hidden')
    vi.advanceTimersByTime(60_000)
    expect(refresh).toHaveBeenCalledOnce()
    subscription.unsubscribe()
    visibility.mockReturnValue('visible')
    vi.advanceTimersByTime(60_000)
    expect(refresh).toHaveBeenCalledOnce()
  })

  it('queues one follow-up read instead of overlapping slow refreshes', async () => {
    vi.useFakeTimers()
    let finish!: () => void
    const refresh = vi.fn<() => void | Promise<void>>()
      .mockImplementationOnce(() => new Promise<void>((resolve) => { finish = resolve }))
    subscribe(refresh)
    emitter.emit('sms_changed', { uris: [] })
    await vi.advanceTimersByTimeAsync(SMS_NOTIFICATION_REFRESH_DELAY_MS)
    emitter.emit('sms_changed', { uris: [] })
    await vi.advanceTimersByTimeAsync(SMS_NOTIFICATION_REFRESH_DELAY_MS)
    expect(refresh).toHaveBeenCalledOnce()
    finish()
    await vi.advanceTimersByTimeAsync(SMS_NOTIFICATION_REFRESH_DELAY_MS)
    expect(refresh).toHaveBeenCalledTimes(2)
  })

  it('drops queued work when unsubscribed during a refresh', async () => {
    vi.useFakeTimers()
    let finish!: () => void
    const refresh = vi.fn(() => new Promise<void>((resolve) => { finish = resolve }))
    const subscription = subscribe(refresh)
    emitter.emit('sms_changed', { uris: [] })
    await vi.advanceTimersByTimeAsync(SMS_NOTIFICATION_REFRESH_DELAY_MS)
    emitter.emit('sms_changed', { uris: [] })
    await vi.advanceTimersByTimeAsync(SMS_NOTIFICATION_REFRESH_DELAY_MS)
    subscription.unsubscribe()
    finish()
    await vi.advanceTimersByTimeAsync(2000)
    expect(refresh).toHaveBeenCalledOnce()
  })

  it('refreshes within two seconds even while provider events continue', () => {
    vi.useFakeTimers()
    const refresh = vi.fn()
    subscribe(refresh)
    for (let elapsed = 0; elapsed < 2000; elapsed += 100) {
      emitter.emit('sms_changed', { uris: ['content://sms/1'] })
      vi.advanceTimersByTime(100)
    }
    expect(refresh).toHaveBeenCalled()
  })

  it('catches up on returning to a visible tab and on network restoration', () => {
    vi.useFakeTimers()
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible')
    const refresh = vi.fn()
    subscribe(refresh)
    document.dispatchEvent(new Event('visibilitychange'))
    window.dispatchEvent(new Event('focus'))
    window.dispatchEvent(new Event('online'))
    vi.advanceTimersByTime(SMS_NOTIFICATION_REFRESH_DELAY_MS)
    expect(refresh).toHaveBeenCalledOnce()
  })

  it('does not refresh on becoming hidden or after lifecycle cleanup', () => {
    vi.useFakeTimers()
    const visibility = vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden')
    const refresh = vi.fn()
    const subscription = subscribe(refresh)
    document.dispatchEvent(new Event('visibilitychange'))
    window.dispatchEvent(new Event('focus'))
    vi.advanceTimersByTime(2000)
    expect(refresh).not.toHaveBeenCalled()
    visibility.mockReturnValue('visible')
    window.dispatchEvent(new Event('focus'))
    subscription.unsubscribe()
    document.dispatchEvent(new Event('visibilitychange'))
    window.dispatchEvent(new Event('online'))
    vi.advanceTimersByTime(2000)
    expect(refresh).not.toHaveBeenCalled()
  })

  it('refreshes after a recognized SMS notification settles', () => {
    vi.useFakeTimers()
    const refresh = vi.fn()
    subscribe(refresh)

    emitter.emit('notification_created', notification('com.google.android.apps.messaging'))

    expect(refresh).not.toHaveBeenCalled()
    vi.advanceTimersByTime(SMS_NOTIFICATION_REFRESH_DELAY_MS)
    expect(refresh).toHaveBeenCalledTimes(1)
  })

  it('coalesces notification create and update events into one refresh', () => {
    vi.useFakeTimers()
    const refresh = vi.fn()
    subscribe(refresh)

    emitter.emit('notification_created', notification('com.android.mms'))
    vi.advanceTimersByTime(SMS_NOTIFICATION_REFRESH_DELAY_MS - 1)
    emitter.emit('notification_updated', notification('com.android.mms'))
    vi.advanceTimersByTime(SMS_NOTIFICATION_REFRESH_DELAY_MS - 1)

    expect(refresh).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)
    expect(refresh).toHaveBeenCalledTimes(1)
  })

  it('ignores notifications from unrelated apps', () => {
    vi.useFakeTimers()
    const refresh = vi.fn()
    subscribe(refresh)

    emitter.emit('notification_created', notification('com.example.calendar'))
    vi.advanceTimersByTime(SMS_NOTIFICATION_REFRESH_DELAY_MS)

    expect(refresh).not.toHaveBeenCalled()
  })

  it('cancels pending refreshes and listeners when unsubscribed', () => {
    vi.useFakeTimers()
    const refresh = vi.fn()
    const subscription = subscribe(refresh)

    emitter.emit('notification_created', notification('com.android.mms'))
    subscription.unsubscribe()
    emitter.emit('notification_updated', notification('com.android.mms'))
    vi.advanceTimersByTime(SMS_NOTIFICATION_REFRESH_DELAY_MS)

    expect(refresh).not.toHaveBeenCalled()
  })

  it('resumes refreshing after a view is reactivated', () => {
    vi.useFakeTimers()
    const refresh = vi.fn()
    const subscription = subscribe(refresh)

    subscription.unsubscribe()
    subscription.subscribe()
    emitter.emit('notification_created', notification('com.android.mms'))
    vi.advanceTimersByTime(SMS_NOTIFICATION_REFRESH_DELAY_MS)

    expect(refresh).toHaveBeenCalledTimes(1)
  })

  it('refreshes independent active consumers', () => {
    vi.useFakeTimers()
    const refreshThread = vi.fn()
    const refreshSidebar = vi.fn()
    subscribe(refreshThread)
    subscribe(refreshSidebar)

    emitter.emit('notification_updated', notification('com.android.mms'))
    vi.advanceTimersByTime(SMS_NOTIFICATION_REFRESH_DELAY_MS)

    expect(refreshThread).toHaveBeenCalledTimes(1)
    expect(refreshSidebar).toHaveBeenCalledTimes(1)
  })

  it('refreshes for provider changes and successful WebSocket reconnects', () => {
    vi.useFakeTimers()
    const refresh = vi.fn()
    subscribe(refresh)

    emitter.emit('sms_changed', { uris: ['content://sms/1'] })
    emitter.emit('app_socket_connection_changed', true)
    vi.advanceTimersByTime(SMS_NOTIFICATION_REFRESH_DELAY_MS)

    expect(refresh).toHaveBeenCalledTimes(2)
  })

  it('reloads contacts only on reconnect, not on notification or provider churn', () => {
    vi.useFakeTimers()
    const refresh = vi.fn()
    const reloadContacts = vi.fn()
    const subscription = createSmsNotificationRefresh(refresh, reloadContacts)
    subscription.subscribe()
    cleanups.push(subscription.unsubscribe)

    emitter.emit('notification_created', notification('com.android.mms'))
    vi.advanceTimersByTime(SMS_NOTIFICATION_REFRESH_DELAY_MS)
    emitter.emit('sms_changed', { uris: ['content://sms/1'] })
    vi.advanceTimersByTime(SMS_NOTIFICATION_REFRESH_DELAY_MS)
    expect(refresh).toHaveBeenCalledTimes(2)
    expect(reloadContacts).not.toHaveBeenCalled()

    emitter.emit('app_socket_connection_changed', true)
    expect(reloadContacts).toHaveBeenCalledOnce()
  })

  it('ignores WebSocket disconnect events', () => {
    vi.useFakeTimers()
    const refresh = vi.fn()
    subscribe(refresh)

    emitter.emit('app_socket_connection_changed', false)
    vi.advanceTimersByTime(SMS_NOTIFICATION_REFRESH_DELAY_MS)

    expect(refresh).not.toHaveBeenCalled()
  })

  it('refreshes when the phone confirms an MMS send', () => {
    vi.useFakeTimers()
    const refresh = vi.fn()
    subscribe(refresh)

    emitter.emit('mms_sent', 'pending-mms-1')
    vi.advanceTimersByTime(SMS_NOTIFICATION_REFRESH_DELAY_MS)

    expect(refresh).toHaveBeenCalledOnce()
  })
})
