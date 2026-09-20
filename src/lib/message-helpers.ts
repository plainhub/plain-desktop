import type { ISms } from '@/lib/interfaces'

export function createPendingSms(body: string, address: string, threadId: string): ISms {
  return {
    id: 'pending_sms_' + Date.now(),
    body,
    address,
    serviceCenter: '',
    sentAt: new Date().toISOString(),
    type: 'SENT',
    threadId,
    subscriptionId: -1,
    isMms: false,
    attachments: [],
    tags: [],
  }
}

export function createPendingMms(
  id: string,
  body: string,
  address: string,
  threadId: string,
  attachments: ISms['attachments'],
): ISms {
  return {
    id,
    body,
    address,
    serviceCenter: '',
    sentAt: new Date().toISOString(),
    type: 'DRAFT',
    threadId,
    subscriptionId: -1,
    isMms: true,
    attachments,
    tags: [],
  }
}

/** Sort items by date without re-parsing date strings inside the comparator. */
export function sortByDate<T extends { sentAt: string }>(items: readonly T[], descending = false): T[] {
  return items
    .map((item) => ({ item, ts: Date.parse(item.sentAt) }))
    .sort((a, b) => (descending ? b.ts - a.ts : a.ts - b.ts))
    .map((entry) => entry.item)
}
