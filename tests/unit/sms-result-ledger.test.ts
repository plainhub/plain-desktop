import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const transport = vi.hoisted(() => ({ field: '', receive: (_result: any) => {} }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe: (field: string, receive: (result: any) => void) => {
    transport.field = field
    transport.receive = receive
    return () => {}
  },
} }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: vi.fn() } }))

import emitter from '../../src/plugins/eventbus'
import { discardSmsSendResult, subscribeSmsSendResults, takeSmsSendResult, SMS_RESULT_LEDGER_TTL_MS } from '../../src/lib/sms-result-ledger'

beforeEach(() => vi.useFakeTimers())
afterEach(() => {
  vi.runAllTimers()
  vi.useRealTimers()
  vi.clearAllMocks()
})

it('delivers both text and attachment outcomes through the same request-ID ledger', () => {
  expect(transport.field).toBe('smsSendResult')
  const handled = vi.fn((result) => result.requestId === 'text-send')
  const stop = subscribeSmsSendResults(handled)
  const text = { requestId: 'text-send', success: true, resultCode: -1 }
  const attachment = { requestId: 'attachment-send', success: false, resultCode: -1001 }
  transport.receive(text)
  transport.receive(attachment)
  expect(handled.mock.calls.map(([result]) => result)).toEqual([text, attachment])
  expect(takeSmsSendResult('text-send')).toBeUndefined()
  expect(takeSmsSendResult('attachment-send')).toEqual(attachment)
  expect(takeSmsSendResult('attachment-send')).toBeUndefined()
  stop()
})

it('retains a result received before its receipt and expires unclaimed results', () => {
  const result = { requestId: 'early-attachment', success: true, resultCode: 0 }
  transport.receive(result)
  expect(takeSmsSendResult(result.requestId)).toEqual(result)
  transport.receive(result)
  vi.advanceTimersByTime(SMS_RESULT_LEDGER_TTL_MS)
  expect(takeSmsSendResult(result.requestId)).toBeUndefined()
})

it('clears the phone confirmation prompt for a closed compose modal without retaining its result', () => {
  discardSmsSendResult('closed-compose')
  transport.receive({ requestId: 'closed-compose', success: true, resultCode: 0 })
  expect(emitter.emit).toHaveBeenCalledWith('tap_phone', '')
  expect(takeSmsSendResult('closed-compose')).toBeUndefined()
})
