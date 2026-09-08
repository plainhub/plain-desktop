import type { IMessage } from '@/lib/interfaces'

export function createPendingSms(body: string, address: string, threadId: string): IMessage {
  return {
    id: 'pending_sms_' + Date.now(),
    body,
    address,
    serviceCenter: '',
    date: new Date().toISOString(),
    type: 2,
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
  attachments: IMessage['attachments'],
): IMessage {
  return {
    id,
    body,
    address,
    serviceCenter: '',
    date: new Date().toISOString(),
    type: 3,
    threadId,
    subscriptionId: -1,
    isMms: true,
    attachments,
    tags: [],
  }
}

/** Sort items by date without re-parsing date strings inside the comparator. */
export function sortByDate<T extends { date: string }>(items: readonly T[], descending = false): T[] {
  return items
    .map((item) => ({ item, ts: Date.parse(item.date) }))
    .sort((a, b) => (descending ? b.ts - a.ts : a.ts - b.ts))
    .map((entry) => entry.item)
}
