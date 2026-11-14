import type { SmsSendResult } from '@/lib/api/subscription-results'
import emitter from '@/plugins/eventbus'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { SMS_SEND_RESULT_TIMEOUT_MS } from '@/lib/sms-send-deadline'

type SmsHandler = (result: SmsSendResult) => boolean
export const SMS_RESULT_LEDGER_TTL_MS = SMS_SEND_RESULT_TIMEOUT_MS + 30000
const MAX_RESULTS = 500
const results = new Map<string, { result: SmsSendResult; timer: ReturnType<typeof setTimeout> }>()
const ignored = new Map<string, ReturnType<typeof setTimeout>>()
const handlers = new Set<SmsHandler>()

function deleteResult(requestId: string) {
  const entry = results.get(requestId)
  if (entry) clearTimeout(entry.timer)
  results.delete(requestId)
}

graphqlSubscriptions.subscribe('smsSendResult', (result) => {
  const requestId = result.requestId
  if (ignored.has(requestId)) {
    emitter.emit('tap_phone', '')
    clearTimeout(ignored.get(requestId))
    ignored.delete(requestId)
    return
  }
  deleteResult(requestId)
  results.set(requestId, { result, timer: setTimeout(() => deleteResult(requestId), SMS_RESULT_LEDGER_TTL_MS) })
  while (results.size > MAX_RESULTS) deleteResult(results.keys().next().value!)
  for (const handler of handlers) {
    if (handler(result)) {
      deleteResult(requestId)
      break
    }
  }
})

export function subscribeSmsSendResults(handler: SmsHandler) {
  handlers.add(handler)
  return () => handlers.delete(handler)
}

export function takeSmsSendResult(requestId: string) {
  const entry = results.get(requestId)
  deleteResult(requestId)
  return entry?.result
}

export function discardSmsSendResult(requestId: string) {
  deleteResult(requestId)
  clearTimeout(ignored.get(requestId))
  ignored.set(requestId, setTimeout(() => ignored.delete(requestId), SMS_RESULT_LEDGER_TTL_MS))
  while (ignored.size > MAX_RESULTS) {
    const oldest = ignored.keys().next().value!
    clearTimeout(ignored.get(oldest))
    ignored.delete(oldest)
  }
}
