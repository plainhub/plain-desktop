import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createSmsNotificationRefresh, SMS_NOTIFICATION_REFRESH_DELAY_MS } from '../../src/hooks/sms-notification-refresh'
import emitter from '../../src/plugins/eventbus'
import type { INotification } from '../../src/lib/interfaces'

let stop: (() => void) | undefined

beforeEach(() => {
  vi.useFakeTimers()
  vi.stubGlobal('document', Object.assign(new EventTarget(), { visibilityState: 'visible' }))
  vi.stubGlobal('window', new EventTarget())
  vi.stubGlobal('navigator', { onLine: true })
})

afterEach(() => {
  stop?.()
  stop = undefined
  vi.clearAllTimers()
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

it('uses the Rust SMS change hint instead of interpreting notification application IDs', async () => {
  const refresh = vi.fn()
  const subscription = createSmsNotificationRefresh(refresh)
  stop = subscription.unsubscribe
  subscription.subscribe()
  emitter.emit('NOTIFICATION_CREATED', { appId: 'com.google.android.apps.messaging' } as INotification)
  emitter.emit('NOTIFICATION_UPDATED', { appId: 'vendor.mms.app' } as INotification)
  await vi.advanceTimersByTimeAsync(SMS_NOTIFICATION_REFRESH_DELAY_MS)
  expect(refresh).not.toHaveBeenCalled()
  emitter.emit('SMS_PROVIDER_CHANGED', { uris: [] })
  emitter.emit('SMS_PROVIDER_CHANGED', { uris: ['content://sms'] })
  await vi.advanceTimersByTimeAsync(SMS_NOTIFICATION_REFRESH_DELAY_MS)
  expect(refresh).toHaveBeenCalledTimes(1)
})

it('cancels pending UI refresh and unsubscribes from change hints when the view closes', async () => {
  const refresh = vi.fn()
  const subscription = createSmsNotificationRefresh(refresh)
  stop = subscription.unsubscribe
  subscription.subscribe()
  emitter.emit('SMS_PROVIDER_CHANGED', { uris: [] })
  subscription.unsubscribe()
  emitter.emit('SMS_PROVIDER_CHANGED', { uris: [] })
  await vi.advanceTimersByTimeAsync(60_000)
  expect(refresh).not.toHaveBeenCalled()
})
