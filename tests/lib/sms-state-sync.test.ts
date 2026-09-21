import { describe, expect, it } from 'vitest'
import type { ISms, ISmsConversation } from '@/lib/interfaces'
import {
  addPendingMms,
  addPendingSms,
  failPendingSms,
  isPendingSmsSent,
  reconcilePendingSms,
  settlePendingMms,
  settlePendingSmsResult,
  visiblePendingSms,
} from '@/lib/sms-state-sync'
import { mergeConversationPage, resolveConversationSendAddress } from '@/lib/sms-conversation-sync'

function message(overrides: Partial<ISms> = {}): ISms {
  return {
    id: 'server-1', body: 'hello', address: '+15551234567', serviceCenter: '',
    date: '2026-08-28T10:00:01.000Z', type: 'SENT', threadId: 'thread-a',
    subscriptionId: 1, tags: [], ...overrides,
  }
}

describe('pending SMS state', () => {
  it('keeps multiple pending sends scoped to their threads across stale refreshes', () => {
    const first = addPendingSms([], 'pending-a', 'first', '+15551234567', 'thread-a', new Date('2026-08-28T10:00:00Z'))
    const pending = addPendingSms(first, 'pending-b', 'second', '+15557654321', 'thread-b', new Date('2026-08-28T10:00:00Z'))

    const afterStaleRefresh = reconcilePendingSms(pending, [message({ body: 'older' })], 'thread-a')

    expect(visiblePendingSms(afterStaleRefresh, 'thread-a').map((item) => item.id)).toEqual(['pending-a'])
    expect(visiblePendingSms(afterStaleRefresh, 'thread-b').map((item) => item.id)).toEqual(['pending-b'])
  })

  it('reconciles one matching provider row without clearing unrelated sends', () => {
    let pending = addPendingSms([], 'pending-a', 'same', '+1 (555) 123-4567', 'thread-a', new Date('2026-08-28T10:00:00Z'))
    pending = addPendingSms(pending, 'pending-b', 'other', '+15551234567', 'thread-a', new Date('2026-08-28T10:00:00Z'))

    const reconciled = reconcilePendingSms(pending, [message({ body: 'same' })], 'thread-a')

    expect(reconciled.map((item) => item.id)).toEqual(['pending-b'])
  })

  it('does not reconcile against a same-body row that existed before the send', () => {
    const pending = addPendingSms(
      [], 'pending-a', 'same', '+15551234567', 'thread-a',
      new Date('2026-08-28T10:00:00Z'), ['server-1'],
    )

    expect(reconcilePendingSms(pending, [message({ body: 'same' })], 'thread-a')).toHaveLength(1)
  })

  it('fails only the send identified by the backend result requestId', () => {
    let pending = addPendingSms([], 'pending-a', 'first', '+15551234567', 'thread-a')
    pending = addPendingSms(pending, 'pending-b', 'second', '+15551234567', 'thread-a')

    const result = failPendingSms(pending, 'pending-a')

    expect(result.failed?.body).toBe('first')
    expect(result.pending.map((item) => item.id)).toEqual(['pending-b'])
  })

  it('matches a country-prefixed provider address to its local form', () => {
    const pending = addPendingSms(
      [], 'pending-a', 'same', '+1 (555) 123-4567', 'thread-a', new Date('2026-08-28T10:00:00Z'),
    )
    expect(reconcilePendingSms(
      pending,
      [message({ body: 'same', address: '5551234567' })],
      'thread-a',
    )).toEqual([])
  })

  it('uses literal matching for alphanumeric addresses instead of collapsing them to empty digits', () => {
    const pending = addPendingSms(
      [], 'pending-a', 'same', 'PlainApp', 'thread-a', new Date('2026-08-28T10:00:00Z'),
    )
    expect(reconcilePendingSms(
      pending,
      [message({ body: 'same', address: 'OtherApp' })],
      'thread-a',
    ).map((item) => item.id)).toEqual(['pending-a'])
    expect(reconcilePendingSms(
      pending,
      [message({ body: 'same', address: 'plainapp' })],
      'thread-a',
    )).toEqual([])
  })

  it('settles a correlated async failure only once so callers toast once', () => {
    const pending = addPendingSms([], 'pending-a', 'first', '+15551234567', 'thread-a')
    const first = settlePendingSmsResult(pending, { requestId: 'pending-a', success: false })
    const duplicate = settlePendingSmsResult(first.pending, { requestId: 'pending-a', success: false })

    expect(first).toMatchObject({ handled: true, failed: { body: 'first' } })
    expect(duplicate).toEqual({ pending: [], handled: false })
  })

  it('marks a correlated successful send as sent while retaining it for provider reconciliation', () => {
    const pending = addPendingSms(
      [], 'pending-a', 'first', '+15551234567', 'thread-a', new Date('2026-08-28T10:00:00Z'),
    )

    const success = settlePendingSmsResult(pending, { requestId: 'pending-a', success: true })

    expect(success.handled).toBe(true)
    expect(success.pending.map((item) => item.id)).toEqual(['pending-a'])
    expect(isPendingSmsSent(success.pending[0])).toBe(true)
    expect(reconcilePendingSms(
      success.pending,
      [message({ body: 'first', date: '2026-08-28T10:00:01.000Z' })],
      'thread-a',
    )).toEqual([])
  })

  it('ignores a duplicate successful terminal result', () => {
    const pending = addPendingSms([], 'pending-a', 'first', '+15551234567', 'thread-a')
    const first = settlePendingSmsResult(pending, { requestId: 'pending-a', success: true })

    expect(settlePendingSmsResult(
      first.pending,
      { requestId: 'pending-a', success: true },
    )).toEqual({ pending: first.pending, handled: true })
  })
})

describe('pending MMS state', () => {
  it('keeps multiple accepted MMS sends and settles reversed outcomes by pending ID', () => {
    let pending = addPendingMms([], message({ id: 'pending-mms-a', body: 'first' }))
    pending = addPendingMms(pending, message({ id: 'pending-mms-b', body: 'second' }))

    const second = settlePendingMms(pending, 'pending-mms-b')
    expect(second.settled?.body).toBe('second')
    expect(second.pending.map((item) => item.id)).toEqual(['pending-mms-a'])
    const first = settlePendingMms(second.pending, 'pending-mms-a')
    expect(first.settled?.body).toBe('first')
    expect(first.pending).toEqual([])
  })
})

function conversation(id: string): ISmsConversation {
  return { id, address: id, addresses: [id], snippet: id, date: '2026-08-28T10:00:00Z', messageCount: 1, read: true }
}

describe('conversation page merging', () => {
  it('deduplicates conversations when offset pages shift after a new thread arrives', () => {
    expect(mergeConversationPage(
      [conversation('a'), conversation('b')],
      [conversation('b'), conversation('c')],
    ).map((item) => item.id)).toEqual(['a', 'b', 'c'])
  })

  it('requires authoritative one-participant metadata before replying to new-schema threads', () => {
    expect(resolveConversationSendAddress(undefined, ['+15551234567'])).toBe('')
    expect(resolveConversationSendAddress(undefined, ['+15551234567', '+15557654321'])).toBe('')
    expect(resolveConversationSendAddress(
      { ...conversation('group'), address: '+15550000001', addresses: ['+15550000001', '+15550000002'] },
      ['+15550000001'],
    )).toBe('')
    expect(resolveConversationSendAddress(
      { ...conversation('single'), address: '+15550000001', addresses: ['+15550000001'] },
      [],
    )).toBe('+15550000001')
  })

  it('does not collapse distinct international participants with the same national suffix', () => {
    expect(resolveConversationSendAddress(undefined, [
      '+1 202 555 0100',
      '+44 202 555 0100',
    ])).toBe('')
  })

  it('allows a unanimous alphanumeric sender without treating all sender IDs as empty digits', () => {
    const legacy = { ...conversation('legacy-alpha'), address: 'PlainApp', addresses: undefined }
    expect(resolveConversationSendAddress(legacy, ['PlainApp', 'plainapp'], true)).toBe('PlainApp')
    expect(resolveConversationSendAddress(legacy, ['PlainApp', 'OtherApp'], true)).toBe('')
  })

  it('keeps legacy replies disabled until loaded rows agree with the scalar address', () => {
    const legacy = { ...conversation('legacy'), address: '+15550000001', addresses: undefined }
    expect(resolveConversationSendAddress(legacy, [])).toBe('')
    expect(resolveConversationSendAddress(legacy, ['+15550000001', '+15550000002'])).toBe('')
    expect(resolveConversationSendAddress(legacy, ['(555) 000-0001'], true)).toBe('+15550000001')
  })
})
