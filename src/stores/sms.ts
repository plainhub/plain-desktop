import { ref, computed } from 'vue'
import { defineStore } from 'pinia'
import {
  initLazyQuery,
  smsConversationsGQL,
  smsConversationsWithAddressesGQL,
  archivedConversationsGQL,
  archivedConversationsWithAddressesGQL,
  smsCountGQL,
  type QueryResponseContext,
} from '@/lib/api/query'
import { initMutation, archiveConversationGQL, unarchiveConversationGQL } from '@/lib/api/mutation'
import type { IMessageConversation } from '@/lib/interfaces'
import emitter from '@/plugins/eventbus'
import { useTempStore } from '@/stores/temp'
import { deleteCachedByPrefix, getCached, setCached } from '@/lib/api/cache'
import { mergeConversationPage } from '@/lib/sms-conversation-sync'

const LIMIT = 50

type SmsCountsCache = { total: number; inbox: number; sent: number; drafts: number }
type ConversationsCache = { conversations: IMessageConversation[]; count: number }
type RequestMode = 'reset' | 'more' | 'archived'
type RequestMeta = { generation: number; mode: RequestMode; query: string; enhanced: boolean }
type ViewMode = 'normal' | 'archived'
type ArchiveTombstone = {
  operationId: number
  generation: number
  mode: ViewMode
  query: string
  confirmed: boolean
}

function isParticipantSchemaError(error: string): boolean {
  const value = error.toLowerCase()
  return value.includes('addresses') && (value.includes('field') || value.includes('validation'))
}

export const useSmsStore = defineStore('sms', () => {
  const conversations = ref<IMessageConversation[]>([])
  const conversationCount = ref(0)
  const typesCount = ref<Map<string, number>>(new Map())
  const q = ref('')
  const noMore = ref(false)
  let generation = 0
  let archiveOperationSequence = 0
  let currentMode: ViewMode = 'normal'
  const participantFieldsSupported = ref<boolean | undefined>()
  const archiveTombstones = new Map<string, ArchiveTombstone>()

  function filterTombstones(items: IMessageConversation[], mode: ViewMode) {
    return items.filter((item) => archiveTombstones.get(item.id)?.mode !== mode)
  }

  function releaseConfirmedTombstones(mode: ViewMode) {
    for (const [id, tombstone] of archiveTombstones) {
      if (tombstone.mode === mode && tombstone.confirmed) archiveTombstones.delete(id)
    }
  }

  function handleNormal(
    data: { smsConversations: IMessageConversation[]; smsConversationCount: number },
    error: string,
    context?: QueryResponseContext,
  ) {
    const meta = context?.meta as RequestMeta | undefined
    if (!meta || meta.generation !== generation) return
    if (error) {
      if (meta.enhanced && isParticipantSchemaError(error)) {
        participantFieldsSupported.value = false
        void fetchNormalLegacy(context?.variables, meta)
      } else {
        emitter.emit('toast', error)
      }
      return
    }
    if (!data) return
    if (meta.enhanced) participantFieldsSupported.value = true

    const incoming = data.smsConversations.map((item) => ({ ...item }))
    const items = filterTombstones(incoming, 'normal')
    noMore.value = incoming.length < (context?.variables?.limit ?? LIMIT)
    if (meta.mode === 'reset') {
      conversations.value = items
      setCached<ConversationsCache>(`sms:conversations:${meta.query}`, {
        conversations: items,
        count: data.smsConversationCount,
      })
      releaseConfirmedTombstones('normal')
    } else {
      conversations.value = mergeConversationPage(conversations.value, items)
    }
    const hiddenIds = new Set(incoming.filter((item) => !items.includes(item)).map((item) => item.id))
    for (const [id, tombstone] of archiveTombstones) {
      if (tombstone.mode === 'normal' && !tombstone.confirmed && tombstone.query === meta.query) hiddenIds.add(id)
    }
    conversationCount.value = Math.max(items.length, data.smsConversationCount - hiddenIds.size)
  }

  const enhancedNormalQuery = initLazyQuery({ handle: handleNormal, document: smsConversationsWithAddressesGQL })
  const legacyNormalQuery = initLazyQuery({ handle: handleNormal, document: smsConversationsGQL })

  function fetchNormalLegacy(variables: Record<string, any> | undefined, previous: RequestMeta) {
    return legacyNormalQuery.fetch(variables, {
      force: true,
      latest: true,
      meta: { ...previous, enhanced: false },
    })
  }

  function handleArchived(
    data: { archivedConversations: IMessageConversation[] },
    error: string,
    context?: QueryResponseContext,
  ) {
    const meta = context?.meta as RequestMeta | undefined
    if (!meta || meta.generation !== generation) return
    if (error) {
      if (meta.enhanced && isParticipantSchemaError(error)) {
        participantFieldsSupported.value = false
        void legacyArchivedQuery.fetch({}, {
          force: true,
          latest: true,
          meta: { ...meta, enhanced: false },
        })
      } else {
        emitter.emit('toast', error)
      }
      return
    }
    if (!data) return
    if (meta.enhanced) participantFieldsSupported.value = true
    const items = filterTombstones(data.archivedConversations.map((item) => ({ ...item })), 'archived')
    conversations.value = items
    conversationCount.value = items.length
    noMore.value = true
    setCached<IMessageConversation[]>('sms:archived', items)
    releaseConfirmedTombstones('archived')
  }

  const enhancedArchivedQuery = initLazyQuery({ handle: handleArchived, document: archivedConversationsWithAddressesGQL })
  const legacyArchivedQuery = initLazyQuery({ handle: handleArchived, document: archivedConversationsGQL })

  const countsQuery = initLazyQuery({
    handle: (data: { smsAllCounts: SmsCountsCache }) => {
      if (data?.smsAllCounts) {
        const counts = data.smsAllCounts
        useTempStore().counter.messages = counts.total
        typesCount.value = new Map([['1', counts.inbox], ['2', counts.sent], ['3', counts.drafts]])
        setCached<SmsCountsCache>('sms:counts', counts)
      }
    },
    document: smsCountGQL,
  })

  const { mutate: mutateArchive } = initMutation({ document: archiveConversationGQL })
  const { mutate: mutateUnarchive } = initMutation({ document: unarchiveConversationGQL })

  function runNormalRequest(mode: 'reset' | 'more', query: string, offset: number, force: boolean, limit = LIMIT) {
    const enhanced = participantFieldsSupported.value !== false
    const request = enhanced ? enhancedNormalQuery : legacyNormalQuery
    return request.fetch(
      { offset, limit, query },
      { force, latest: true, meta: { generation, mode, query, enhanced } satisfies RequestMeta },
    )
  }

  function fetchConversations(query = '', reset = true, force = false) {
    const limit = reset && force && currentMode === 'normal' && query === q.value
      ? Math.max(LIMIT, conversations.value.length)
      : LIMIT
    if (reset) {
      generation++
      currentMode = 'normal'
      q.value = query
      if (!force) {
        const cached = getCached<ConversationsCache>(`sms:conversations:${query}`)
        if (cached) {
          conversations.value = filterTombstones(cached.conversations, 'normal')
          conversationCount.value = cached.count
          noMore.value = cached.conversations.length >= cached.count
        } else {
          conversations.value = []
          noMore.value = false
        }
      }
    }
    return runNormalRequest(reset ? 'reset' : 'more', query, reset ? 0 : conversations.value.length, force, limit)
  }

  function fetchMoreConversations() {
    if (noMore.value || normalLoading.value) return
    return fetchConversations(q.value, false, false)
  }

  function fetchArchived(force = false) {
    generation++
    currentMode = 'archived'
    if (!force) {
      const cached = getCached<IMessageConversation[]>('sms:archived')
      if (cached) {
        conversations.value = filterTombstones(cached, 'archived')
        conversationCount.value = cached.length
        noMore.value = true
      } else {
        conversations.value = []
        noMore.value = false
      }
    }
    const enhanced = participantFieldsSupported.value !== false
    const request = enhanced ? enhancedArchivedQuery : legacyArchivedQuery
    return request.fetch({}, {
      force,
      latest: true,
      meta: { generation, mode: 'archived', query: '', enhanced } satisfies RequestMeta,
    })
  }

  function fetchCounts(force = false) {
    if (!force) {
      const cached = getCached<SmsCountsCache>('sms:counts')
      if (cached) {
        useTempStore().counter.messages = cached.total
        typesCount.value = new Map([['1', cached.inbox], ['2', cached.sent], ['3', cached.drafts]])
      }
    }
    return countsQuery.fetch({}, { force, latest: true })
  }

  async function archiveConversations(ids: string[]) {
    const operationId = ++archiveOperationSequence
    const operationGeneration = generation
    const operationMode = currentMode
    const operationQuery = q.value
    const originals = ids.flatMap((id) => {
      const index = conversations.value.findIndex((item) => item.id === id)
      return index < 0 ? [] : [{ item: conversations.value[index], index }]
    })
    const idSet = new Set(ids)
    for (const id of ids) {
      archiveTombstones.set(id, {
        operationId,
        generation: operationGeneration,
        mode: operationMode,
        query: operationQuery,
        confirmed: false,
      })
    }
    conversations.value = conversations.value.filter((item) => !idSet.has(item.id))
    conversationCount.value = Math.max(0, conversationCount.value - originals.length)
    deleteCachedByPrefix('sms:conversations:')
    deleteCachedByPrefix('sms:archived')

    const date = Date.now()
    const results = await Promise.all(ids.map(async (id) => {
      const result = await mutateArchive({ id, date })
      const original = originals.find(({ item }) => item.id === id)
      const tombstone = archiveTombstones.get(id)
      if (result != null) {
        if (tombstone?.operationId === operationId) tombstone.confirmed = true
      } else {
        const canRollback = generation === operationGeneration
          && currentMode === operationMode
          && q.value === operationQuery
          && tombstone?.operationId === operationId
          && tombstone?.generation === operationGeneration
        if (tombstone?.operationId === operationId) archiveTombstones.delete(id)
        if (canRollback && original && !conversations.value.some((current) => current.id === id)) {
          const restored = [...conversations.value]
          restored.splice(Math.min(original.index, restored.length), 0, original.item)
          conversations.value = restored
          conversationCount.value++
        }
      }
      return result != null
    }))
    if (currentMode === 'archived') void fetchArchived(true)
    else void fetchConversations(q.value, true, true)
    return results.every(Boolean)
  }

  async function unarchiveConversations(ids: string[]) {
    const operationId = ++archiveOperationSequence
    const operationGeneration = generation
    const operationMode = currentMode
    const operationQuery = q.value
    const originals = ids.flatMap((id) => {
      const index = conversations.value.findIndex((item) => item.id === id)
      return index < 0 ? [] : [{ item: conversations.value[index], index }]
    })
    const idSet = new Set(ids)
    for (const id of ids) {
      archiveTombstones.set(id, {
        operationId,
        generation: operationGeneration,
        mode: operationMode,
        query: operationQuery,
        confirmed: false,
      })
    }
    conversations.value = conversations.value.filter((item) => !idSet.has(item.id))
    conversationCount.value = Math.max(0, conversationCount.value - originals.length)
    deleteCachedByPrefix('sms:conversations:')
    deleteCachedByPrefix('sms:archived')

    const results = await Promise.all(ids.map(async (id) => {
      const result = await mutateUnarchive({ id })
      const original = originals.find(({ item }) => item.id === id)
      const tombstone = archiveTombstones.get(id)
      if (result != null) {
        if (tombstone?.operationId === operationId) tombstone.confirmed = true
      } else {
        const canRollback = generation === operationGeneration
          && currentMode === operationMode
          && q.value === operationQuery
          && tombstone?.operationId === operationId
          && tombstone?.generation === operationGeneration
        if (tombstone?.operationId === operationId) archiveTombstones.delete(id)
        if (canRollback && original && !conversations.value.some((current) => current.id === id)) {
          const restored = [...conversations.value]
          restored.splice(Math.min(original.index, restored.length), 0, original.item)
          conversations.value = restored
          conversationCount.value++
        }
      }
      return result != null
    }))
    if (currentMode === 'archived') void fetchArchived(true)
    else void fetchConversations(q.value, true, true)
    return results.every(Boolean)
  }

  const normalLoading = computed(() => enhancedNormalQuery.loading.value || legacyNormalQuery.loading.value)
  const archivedLoading = computed(() => enhancedArchivedQuery.loading.value || legacyArchivedQuery.loading.value)
  const loading = computed(() => normalLoading.value || archivedLoading.value)

  return {
    conversations,
    conversationCount,
    typesCount,
    noMore,
    loading,
    normalLoading,
    archivedLoading,
    participantFieldsSupported,
    fetchConversations,
    fetchMoreConversations,
    fetchArchived,
    fetchCounts,
    archiveConversations,
    unarchiveConversations,
  }
})
