import { afterEach, describe, expect, it, vi } from 'vitest'
import emitter from '@/plugins/eventbus'
import {
  discardMmsSendResult,
  SMS_RESULT_LEDGER_TTL_MS,
  subscribeSmsSendResults,
  takeMmsSendResult,
  takeSmsSendResult,
} from '@/lib/sms-result-ledger'

afterEach(() => vi.useRealTimers())

describe('SMS result ledger', () => {
  it('queues a correlated SMS result while its thread has no active listener', () => {
    emitter.emit('SMS_SEND_RESULT', { clientId: 'browser-a', requestId: 'inactive-sms', success: false, resultCode: 1 })

    expect(takeSmsSendResult('inactive-sms')).toEqual({
      clientId: 'browser-a', requestId: 'inactive-sms', success: false, resultCode: 1,
    })
  })

  it('delivers active results once without leaving a queued duplicate', () => {
    const handler = vi.fn(() => true)
    const unsubscribe = subscribeSmsSendResults(handler)

    emitter.emit('SMS_SEND_RESULT', { clientId: 'browser-a', requestId: 'active-sms', success: true })
    unsubscribe()

    expect(handler).toHaveBeenCalledOnce()
    expect(takeSmsSendResult('active-sms')).toBeUndefined()
  })

  it('keeps concurrent sends from one client separated by request ID', () => {
    emitter.emit('SMS_SEND_RESULT', { clientId: 'browser-a', requestId: 'send-a', success: true })
    emitter.emit('SMS_SEND_RESULT', { clientId: 'browser-a', requestId: 'send-b', success: false, resultCode: 4 })

    expect(takeSmsSendResult('send-b')).toMatchObject({ requestId: 'send-b', success: false })
    expect(takeSmsSendResult('send-a')).toMatchObject({ requestId: 'send-a', success: true })
  })

  it('queues an MMS timeout for its inactive originating thread', () => {
    emitter.emit('MMS_SEND_RESULT', { pendingId: 'inactive-mms', success: false, resultCode: -1000 })

    expect(takeMmsSendResult('inactive-mms')).toEqual({
      pendingId: 'inactive-mms', success: false, resultCode: -1000,
    })
  })

  it('queues legacy MMS success while its originating thread is inactive', () => {
    emitter.emit('MMS_SENT', 'successful-mms')

    expect(takeMmsSendResult('successful-mms')).toEqual({
      pendingId: 'successful-mms', success: true,
    })
  })

  it('expires orphaned results after a bounded window longer than backend completion', () => {
    vi.useFakeTimers()
    emitter.emit('SMS_SEND_RESULT', { clientId: 'browser-a', requestId: 'orphan-sms', success: true })
    vi.advanceTimersByTime(SMS_RESULT_LEDGER_TTL_MS)
    expect(takeSmsSendResult('orphan-sms')).toBeUndefined()
  })

  it('lets modal MMS flows explicitly discard a result with no optimistic owner', () => {
    emitter.emit('MMS_SENT', 'modal-mms')
    discardMmsSendResult('modal-mms')
    expect(takeMmsSendResult('modal-mms')).toBeUndefined()
  })
})
