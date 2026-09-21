import { afterEach, describe, expect, it, vi } from 'vitest'
import { nextTick, ref } from 'vue'
import { SMS_SEND_RESULT_TIMEOUT_MS } from '@/lib/sms-send-deadline'
import { isPendingSmsSent } from '@/lib/sms-state-sync'

const harness = vi.hoisted(() => ({ toast: vi.fn(), fetch: vi.fn(), handle: undefined as any }))

vi.mock('@/components/toaster', () => ({ default: harness.toast }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/lib/api/query', async () => {
  const { ref } = await vi.importActual<typeof import('vue')>('vue')
  return {
    smsGQL: 'sms-query',
    initLazyQuery: (params: any) => {
      harness.handle = params.handle
      return { loading: ref(false), fetch: harness.fetch }
    },
  }
})
vi.mock('@/hooks/tags', async () => {
  const { ref } = await vi.importActual<typeof import('vue')>('vue')
  return { useTags: () => ({ tags: ref([]), fetch: vi.fn() }) }
})
vi.mock('@/hooks/contacts', () => ({
  useContactName: () => ({ loadContacts: vi.fn(), getDisplayName: (value: string[]) => value.join(', ') }),
}))
vi.mock('@/hooks/sms-notification-refresh', () => ({
  createSmsNotificationRefresh: () => ({ subscribe: vi.fn(), unsubscribe: vi.fn() }),
}))

import { useMessageThread } from '@/hooks/message-thread'

afterEach(() => {
  harness.toast.mockReset()
  harness.fetch.mockReset()
  vi.useRealTimers()
})

describe('message thread pending operations', () => {
  it('retains the loaded history window and reading position on background refresh', async () => {
    const scroll = { scrollTop: 250, scrollHeight: 2000, clientHeight: 500 } as HTMLElement
    const thread = useMessageThread(ref('thread-a'), ref(scroll))
    thread.setPendingSms('hello', '+15551234567', 'request-a')
    await nextTick()
    scroll.scrollTop = 250
    thread.items.value = Array.from({ length: 200 }, (_, i) => ({
      ...thread.pendingSmsItems.value[0], id: String(i),
    }))
    await thread.fetch(true, true)
    const [variables, options] = harness.fetch.mock.calls[0]
    expect(variables.limit).toBe(200)
    harness.handle({ sms: thread.items.value, smsCount: 200 }, '', { variables, meta: options.meta })
    await nextTick()
    expect(scroll.scrollTop).toBe(250)
  })

  it('orders pending SMS and MMS together with provider messages', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-09-17T13:58:00Z'))
    const thread = useMessageThread(ref('thread-a'), ref())
    thread.setPendingSms('earlier SMS', '+15551234567', 'request-a')
    vi.setSystemTime(new Date('2026-09-17T13:57:00Z'))
    thread.setPendingMms('mms-a', 'earlier MMS', '+15551234567', [])
    thread.items.value = [{
      ...thread.pendingSmsItems.value[0],
      id: 'provider-a',
      sentAt: '2026-09-17T13:59:00Z',
    }]

    expect(thread.sortedItems.value.map((item) => item.id)).toEqual(['mms-a', 'request-a', 'provider-a'])
    expect(thread.items.value.map((item) => item.id)).toEqual(['provider-a'])
  })

  it('toasts once when a correlated asynchronous SMS failure is duplicated', () => {
    const thread = useMessageThread(ref('thread-a'), ref())
    const requestId = thread.setPendingSms('hello', '+15551234567', 'request-a')

    expect(thread.handleSmsSendResult({ requestId, success: false }).handled).toBe(true)
    expect(thread.handleSmsSendResult({ requestId, success: false }).handled).toBe(false)
    expect(harness.toast).toHaveBeenCalledOnce()
    expect(harness.toast).toHaveBeenCalledWith('send_failed', 'error')
  })

  it('fails and restores only the SMS whose backend deadline expires', () => {
    vi.useFakeTimers()
    const thread = useMessageThread(ref('thread-a'), ref())
    const restore = vi.fn()
    thread.setTerminalHandlers({ onSmsFailure: restore, onMmsResult: vi.fn() })
    thread.setPendingSms('first', '+15551234567', 'request-a')
    thread.startPendingSmsDeadline('request-a')
    vi.advanceTimersByTime(1000)
    thread.setPendingSms('second', '+15551234567', 'request-b')
    thread.startPendingSmsDeadline('request-b')
    thread.handleSmsSendResult({ requestId: 'request-a', success: true })

    vi.advanceTimersByTime(SMS_SEND_RESULT_TIMEOUT_MS)

    expect(restore).toHaveBeenCalledOnce()
    expect(restore.mock.calls[0][0].body).toBe('second')
    expect(thread.pendingSmsItems.value.map((item) => item.id)).toEqual(['request-a'])
    expect(harness.toast).toHaveBeenCalledOnce()
  })

  it('does not start a deadline after a success result beats mutation acceptance', async () => {
    vi.useFakeTimers()
    const thread = useMessageThread(ref('thread-a'), ref())
    const restore = vi.fn()
    thread.setTerminalHandlers({ onSmsFailure: restore, onMmsResult: vi.fn() })
    const requestId = thread.setPendingSms('hello', '+15551234567', 'request-a')

    expect(thread.handleSmsSendResult({ requestId, success: true }).handled).toBe(true)
    thread.startPendingSmsDeadline(requestId)
    await vi.advanceTimersByTimeAsync(SMS_SEND_RESULT_TIMEOUT_MS)

    expect(thread.pendingSmsItems.value.map((item) => item.id)).toEqual([requestId])
    expect(isPendingSmsSent(thread.pendingSmsItems.value[0])).toBe(true)
    expect(restore).not.toHaveBeenCalled()
    expect(harness.toast).not.toHaveBeenCalled()
  })

  it('keeps a successful send marked sent when provider retries cannot see a restricted row', async () => {
    vi.useFakeTimers()
    harness.fetch.mockResolvedValue(undefined)
    const thread = useMessageThread(ref('thread-a'), ref())
    const restore = vi.fn()
    thread.setTerminalHandlers({ onSmsFailure: restore, onMmsResult: vi.fn() })
    const requestId = thread.setPendingSms('hello', '+15551234567', 'request-a')
    thread.startPendingSmsDeadline(requestId)

    expect(thread.handleSmsSendResult({ requestId, success: true }).handled).toBe(true)
    await vi.advanceTimersByTimeAsync(7000)

    expect(harness.fetch).toHaveBeenCalledTimes(3)
    expect(thread.pendingSmsItems.value.map((item) => item.id)).toEqual([requestId])
    expect(isPendingSmsSent(thread.pendingSmsItems.value[0])).toBe(true)
    expect(restore).not.toHaveBeenCalled()
    expect(harness.toast).not.toHaveBeenCalled()
  })

  it('keeps two accepted MMS sends until reversed terminal outcomes identify each one', () => {
    const thread = useMessageThread(ref('thread-a'), ref())
    thread.setPendingMms('mms-a', 'first', '+15551234567', [])
    thread.setPendingMms('mms-b', 'second', '+15551234567', [])
    expect(thread.pendingMmsItems.value.map((item) => item.id)).toEqual(['mms-a', 'mms-b'])

    expect(thread.handleMmsSendResult({ pendingId: 'mms-b', success: false }).failed?.body).toBe('second')
    expect(thread.pendingMmsItems.value.map((item) => item.id)).toEqual(['mms-a'])
    expect(thread.handleMmsSendResult({ pendingId: 'mms-a', success: true }).handled).toBe(true)
    expect(thread.pendingMmsItems.value).toEqual([])
  })
})
