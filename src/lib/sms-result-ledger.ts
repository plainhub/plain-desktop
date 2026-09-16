import type { IMmsSendResultEvent, ISmsSendResultEvent } from '@/lib/interfaces'
import emitter from '@/plugins/eventbus'
import { SMS_SEND_RESULT_TIMEOUT_MS } from '@/lib/sms-send-deadline'

type SmsHandler = (result: ISmsSendResultEvent) => boolean
type MmsHandler = (result: IMmsSendResultEvent) => boolean

export const SMS_RESULT_LEDGER_TTL_MS = SMS_SEND_RESULT_TIMEOUT_MS + 30000
const MAX_RESULTS = 500

type LedgerEntry<T> = { result: T; timer: ReturnType<typeof setTimeout> }

const smsResults = new Map<string, LedgerEntry<ISmsSendResultEvent>>()
const mmsResults = new Map<string, LedgerEntry<IMmsSendResultEvent>>()
const ignoredMmsResults = new Map<string, ReturnType<typeof setTimeout>>()
const smsHandlers = new Set<SmsHandler>()
const mmsHandlers = new Set<MmsHandler>()

function deleteSmsResult(requestId: string) {
  const entry = smsResults.get(requestId)
  if (entry) clearTimeout(entry.timer)
  smsResults.delete(requestId)
}

function deleteMmsResult(pendingId: string) {
  const entry = mmsResults.get(pendingId)
  if (entry) clearTimeout(entry.timer)
  mmsResults.delete(pendingId)
}

function enforceBound<T>(results: Map<string, LedgerEntry<T>>, remove: (id: string) => void) {
  while (results.size > MAX_RESULTS) {
    const oldest = results.keys().next().value as string | undefined
    if (!oldest) return
    remove(oldest)
  }
}

function recordSmsResult(result: ISmsSendResultEvent) {
  const requestId = result.requestId!
  deleteSmsResult(requestId)
  smsResults.set(requestId, {
    result,
    timer: setTimeout(() => deleteSmsResult(requestId), SMS_RESULT_LEDGER_TTL_MS),
  })
  enforceBound(smsResults, deleteSmsResult)
}

function recordMmsResult(result: IMmsSendResultEvent) {
  if (ignoredMmsResults.has(result.pendingId)) {
    const timer = ignoredMmsResults.get(result.pendingId)
    if (timer) clearTimeout(timer)
    ignoredMmsResults.delete(result.pendingId)
    return
  }
  deleteMmsResult(result.pendingId)
  mmsResults.set(result.pendingId, {
    result,
    timer: setTimeout(() => deleteMmsResult(result.pendingId), SMS_RESULT_LEDGER_TTL_MS),
  })
  enforceBound(mmsResults, deleteMmsResult)
}

emitter.on('sms_send_result', (result) => {
  if (!result.requestId) return
  recordSmsResult(result)
  for (const handler of smsHandlers) {
    if (handler(result)) {
      deleteSmsResult(result.requestId)
      break
    }
  }
})

emitter.on('mms_send_result', (result) => {
  recordMmsResult(result)
  // The phone finished the MMS — the "confirm on phone" banner is obsolete.
  emitter.emit('tap_phone', '')
  for (const handler of mmsHandlers) {
    if (handler(result)) {
      deleteMmsResult(result.pendingId)
      break
    }
  }
})

// Event 17 is the legacy/success half of the MMS result contract. Recording
// it here prevents a success from being lost while its route is deactivated.
emitter.on('mms_sent', (pendingId) => {
  emitter.emit('tap_phone', '')
  const result: IMmsSendResultEvent = { pendingId, success: true }
  recordMmsResult(result)
  for (const handler of mmsHandlers) {
    if (handler(result)) {
      deleteMmsResult(pendingId)
      break
    }
  }
})

export function subscribeSmsSendResults(handler: SmsHandler) {
  smsHandlers.add(handler)
  return () => smsHandlers.delete(handler)
}

export function subscribeMmsSendResults(handler: MmsHandler) {
  mmsHandlers.add(handler)
  return () => mmsHandlers.delete(handler)
}

export function takeSmsSendResult(requestId: string) {
  const entry = smsResults.get(requestId)
  deleteSmsResult(requestId)
  return entry?.result
}

export function takeMmsSendResult(pendingId: string) {
  const entry = mmsResults.get(pendingId)
  deleteMmsResult(pendingId)
  return entry?.result
}

export function discardMmsSendResult(pendingId: string) {
  deleteMmsResult(pendingId)
  const existing = ignoredMmsResults.get(pendingId)
  if (existing) clearTimeout(existing)
  ignoredMmsResults.set(pendingId, setTimeout(() => {
    ignoredMmsResults.delete(pendingId)
  }, SMS_RESULT_LEDGER_TTL_MS))
  while (ignoredMmsResults.size > MAX_RESULTS) {
    const oldest = ignoredMmsResults.keys().next().value as string | undefined
    if (!oldest) break
    const timer = ignoredMmsResults.get(oldest)
    if (timer) clearTimeout(timer)
    ignoredMmsResults.delete(oldest)
  }
}
