import type { ISms, ISmsSendResultEvent } from '@/lib/interfaces'
import { createPendingSms } from '@/lib/message-helpers'

const MATCH_WINDOW_MS = 5 * 60 * 1000

interface PendingSms extends ISms {
  baselineIds?: string[]
  sendState?: 'sending' | 'sent'
}

export function isPendingSmsSent(item: ISms | undefined): boolean {
  return Boolean(item && (item as PendingSms).sendState === 'sent')
}

export function addressesMatch(first: string, second: string): boolean {
  const a = first.trim().toLowerCase()
  const b = second.trim().toLowerCase()
  if (!a || !b) return false
  if (a === b) return true

  const phonePattern = /^[+\d\s().-]+$/
  if (!phonePattern.test(a) || !phonePattern.test(b)) return false
  const aDigits = a.replace(/\D/g, '')
  const bDigits = b.replace(/\D/g, '')
  if (aDigits === bDigits && aDigits.length >= 7) return true

  const shorter = aDigits.length <= bDigits.length ? aDigits : bDigits
  const longer = aDigits.length > bDigits.length ? aDigits : bDigits
  const prefixLength = longer.length - shorter.length
  return shorter.length >= 10
    && longer.length <= 15
    && prefixLength >= 1
    && prefixLength <= 3
    && longer.endsWith(shorter)
}

export function addPendingSms(
  pending: ISms[],
  requestId: string,
  body: string,
  address: string,
  threadId: string,
  createdAt = new Date(),
  baselineIds: Iterable<string> = [],
): ISms[] {
  return [
    ...pending,
    {
      ...createPendingSms(body, address, threadId),
      id: requestId,
      date: createdAt.toISOString(),
      baselineIds: [...baselineIds],
      sendState: 'sending',
    } as PendingSms,
  ]
}

export function visiblePendingSms(pending: ISms[], threadId: string): ISms[] {
  return pending.filter((item) => item.threadId === threadId)
}

export function reconcilePendingSms(pending: ISms[], confirmed: ISms[], threadId: string): ISms[] {
  const available = confirmed.filter((item) => item.threadId === threadId && item.type === 'SENT')
  const matchedIds = new Set<string>()
  const usedConfirmedIds = new Set<string>()

  for (const operation of pending.filter((item) => item.threadId === threadId)) {
    const operationTime = new Date(operation.date).getTime()
    const baselineIds = new Set((operation as PendingSms).baselineIds ?? [])
    const match = available.find((item) => {
      if (usedConfirmedIds.has(item.id) || baselineIds.has(item.id)) return false
      const itemTime = new Date(item.date).getTime()
      return item.body === operation.body
        && addressesMatch(item.address, operation.address)
        && Math.abs(itemTime - operationTime) <= MATCH_WINDOW_MS
    })
    if (match) {
      matchedIds.add(operation.id)
      usedConfirmedIds.add(match.id)
    }
  }

  return pending.filter((item) => !matchedIds.has(item.id))
}

export function failPendingSms(pending: ISms[], requestId: string): { pending: ISms[]; failed?: ISms } {
  return {
    pending: pending.filter((item) => item.id !== requestId),
    failed: pending.find((item) => item.id === requestId),
  }
}

export function settlePendingSmsResult(
  pending: ISms[],
  result: ISmsSendResultEvent,
): { pending: ISms[]; handled: boolean; transitioned?: true; failed?: ISms } {
  const operation = result.requestId
    ? pending.find((item) => item.id === result.requestId)
    : undefined
  if (!result.requestId || !operation) {
    return { pending, handled: false }
  }
  if (isPendingSmsSent(operation)) return { pending, handled: true }
  if (result.success) {
    return {
      pending: pending.map((item) => item.id === result.requestId
        ? { ...item, sendState: 'sent' } as PendingSms
        : item),
      handled: true,
      transitioned: true,
    }
  }
  return { ...failPendingSms(pending, result.requestId), handled: true, transitioned: true }
}

export function addPendingMms(pending: ISms[], item: ISms): ISms[] {
  return [...pending.filter((current) => current.id !== item.id), item]
}

export function settlePendingMms(
  pending: ISms[],
  pendingId: string,
): { pending: ISms[]; settled?: ISms } {
  return {
    pending: pending.filter((item) => item.id !== pendingId),
    settled: pending.find((item) => item.id === pendingId),
  }
}
