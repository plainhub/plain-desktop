import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import type { ISmsConversation } from '@/lib/interfaces'

const harness = vi.hoisted(() => ({
  queries: [] as Array<{ params: any; fetch: ReturnType<typeof vi.fn> }>,
  archiveSucceeds: true,
  unarchiveSucceeds: true,
  archiveDeferred: undefined as Promise<any> | undefined,
}))

vi.mock('@/lib/api/query', async () => {
  const { ref } = await vi.importActual<typeof import('vue')>('vue')
  return {
    smsConversationsGQL: 'normal-legacy',
    smsConversationsWithAddressesGQL: 'normal-enhanced',
    archivedConversationsGQL: 'archived-legacy',
    archivedConversationsWithAddressesGQL: 'archived-enhanced',
    smsCountGQL: 'counts',
    initLazyQuery: (params: any) => {
      const fetch = vi.fn()
      harness.queries.push({ params, fetch })
      return { loading: ref(false), fetch }
    },
  }
})

vi.mock('@/lib/api/mutation', () => ({
  archiveConversationGQL: 'archive',
  unarchiveConversationGQL: 'unarchive',
  initMutation: ({ document }: { document: string }) => ({
    mutate: vi.fn(async () => {
      if (document === 'archive' && harness.archiveDeferred) return harness.archiveDeferred
      const succeeds = document === 'archive' ? harness.archiveSucceeds : harness.unarchiveSucceeds
      return succeeds ? { data: true } : undefined
    }),
  }),
}))

import { useSmsStore } from '@/stores/sms'

function conversation(id: string): ISmsConversation {
  return {
    id, address: `+1${id.padStart(10, '0')}`, addresses: [`+1${id.padStart(10, '0')}`],
    snippet: id, date: '2026-08-28T10:00:00Z', messageCount: 1, read: true,
  }
}

function normalQuery() {
  return harness.queries.find((entry) => entry.params.document === 'normal-enhanced')!
}

function archivedQuery() {
  return harness.queries.find((entry) => entry.params.document === 'archived-enhanced')!
}

function resolveRequest(callIndex: number, items: ISmsConversation[], count = items.length) {
  const query = normalQuery()
  const call = query.fetch.mock.calls[callIndex]
  query.params.handle(
    { smsConversations: items, smsConversationCount: count },
    '',
    { variables: call[0], requestId: callIndex + 1, meta: call[1].meta },
  )
}

function resolveArchivedRequest(callIndex: number, items: ISmsConversation[]) {
  const query = archivedQuery()
  const call = query.fetch.mock.calls[callIndex]
  query.params.handle(
    { archivedConversations: items },
    '',
    { variables: call[0], requestId: callIndex + 1, meta: call[1].meta },
  )
}

describe('SMS conversation store synchronization', () => {
  beforeEach(() => {
    harness.queries = []
    harness.archiveSucceeds = true
    harness.unarchiveSucceeds = true
    harness.archiveDeferred = undefined
    setActivePinia(createPinia())
  })

  it('retries the same offset after a failed load-more and deduplicates shifted rows', () => {
    const store = useSmsStore()
    const firstPage = Array.from({ length: 50 }, (_, index) => conversation(String(index)))
    store.fetchConversations('')
    resolveRequest(0, firstPage, 75)

    store.fetchMoreConversations()
    const pageRequest = normalQuery().fetch.mock.calls[1]
    expect(pageRequest[0].offset).toBe(50)
    normalQuery().params.handle(undefined, 'network_error', {
      variables: pageRequest[0], requestId: 2, meta: pageRequest[1].meta,
    })

    store.fetchMoreConversations()
    expect(normalQuery().fetch.mock.calls[2][0].offset).toBe(50)
    resolveRequest(2, [conversation('49'), conversation('50')], 51)
    expect(store.conversations.filter((item) => item.id === '49')).toHaveLength(1)
    expect(store.conversations).toHaveLength(51)
  })

  it('ignores a response from the previous filter generation', () => {
    const store = useSmsStore()
    store.fetchConversations('old')
    store.fetchConversations('new')
    resolveRequest(1, [conversation('2')])
    resolveRequest(0, [conversation('1')])

    expect(store.conversations.map((item) => item.id)).toEqual(['2'])
  })

  it('falls back to the legacy conversation query on old schemas', () => {
    const store = useSmsStore()
    store.fetchConversations('legacy')
    const enhanced = normalQuery()
    const call = enhanced.fetch.mock.calls[0]
    enhanced.params.handle(undefined, 'Cannot query field "addresses" on type "SmsConversation"', {
      variables: call[0], requestId: 1, meta: call[1].meta,
    })

    const legacy = harness.queries.find((entry) => entry.params.document === 'normal-legacy')!
    expect(legacy.fetch).toHaveBeenCalledWith(
      { offset: 0, limit: 50, query: 'legacy' },
      expect.objectContaining({ force: true, latest: true, meta: expect.objectContaining({ enhanced: false }) }),
    )
  })

  it('rolls an optimistic archive back and reports failure', async () => {
    const store = useSmsStore()
    store.fetchConversations('')
    resolveRequest(0, [conversation('1')])
    harness.archiveSucceeds = false

    await expect(store.archiveConversations(['1'])).resolves.toBe(false)
    expect(store.conversations.map((item) => item.id)).toEqual(['1'])
    expect(store.conversationCount).toBe(1)
  })

  it('keeps an archived row tombstoned across a concurrent refresh and successful mutation', async () => {
    let resolveArchive!: (value: any) => void
    harness.archiveDeferred = new Promise((resolve) => { resolveArchive = resolve })
    const store = useSmsStore()
    store.fetchConversations('')
    resolveRequest(0, [conversation('1')])

    const archive = store.archiveConversations(['1'])
    store.fetchConversations('', true, true)
    resolveRequest(1, [conversation('1')])
    expect(store.conversations).toEqual([])

    resolveArchive({ data: true })
    await expect(archive).resolves.toBe(true)
    expect(normalQuery().fetch).toHaveBeenCalledTimes(3)
    resolveRequest(2, [conversation('1')])
    expect(store.conversations).toEqual([])
  })

  it('does not roll a failed archive into a newer query generation', async () => {
    let resolveArchive!: (value: any) => void
    harness.archiveDeferred = new Promise((resolve) => { resolveArchive = resolve })
    const store = useSmsStore()
    store.fetchConversations('')
    resolveRequest(0, [conversation('1')])

    const archive = store.archiveConversations(['1'])
    store.fetchConversations('other', true, true)
    resolveRequest(1, [conversation('2')])
    resolveArchive(undefined)

    await expect(archive).resolves.toBe(false)
    expect(store.conversations.map((item) => item.id)).toEqual(['2'])
    expect(normalQuery().fetch.mock.calls[2][0].query).toBe('other')
  })

  it('does not mistake a full page filtered by a pending tombstone for the last page', async () => {
    let resolveArchive!: (value: any) => void
    harness.archiveDeferred = new Promise((resolve) => { resolveArchive = resolve })
    const store = useSmsStore()
    const firstPage = Array.from({ length: 50 }, (_, index) => conversation(String(index)))
    store.fetchConversations('')
    resolveRequest(0, firstPage, 75)

    const archive = store.archiveConversations(['0'])
    store.fetchConversations('', true, true)
    resolveRequest(1, firstPage, 75)

    expect(store.conversations).toHaveLength(49)
    expect(store.noMore).toBe(false)
    resolveArchive({ data: true })
    await archive
  })

  it('rolls an optimistic unarchive back in the same archived generation', async () => {
    const store = useSmsStore()
    store.fetchArchived()
    resolveArchivedRequest(0, [conversation('1')])
    harness.unarchiveSucceeds = false

    await expect(store.unarchiveConversations(['1'])).resolves.toBe(false)
    expect(store.conversations.map((item) => item.id)).toEqual(['1'])
    expect(store.conversationCount).toBe(1)
  })
})
