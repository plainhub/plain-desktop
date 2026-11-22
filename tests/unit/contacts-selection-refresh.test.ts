import { expect, it, vi } from 'vitest'
import { h, KeepAlive, ref } from 'vue'
import emitter from '@/plugins/eventbus'
import { DataType } from '@/lib/data'
import { renderView } from '../helpers/vue-node-renderer'
import { useContactsData } from '@/views/contacts/hooks/useContactsData'

const state = vi.hoisted(() => ({
  rows: [{ id: 'a' }, { id: 'b' }, { id: 'c' }],
  mutate: vi.fn().mockResolvedValue({ data: {} }),
}))
vi.mock('vue-router', () => ({ useRoute: () => ({ path: '/contacts', fullPath: '/contacts', query: {} }) }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({ pageSize: 20 }) }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({}) }) }))
vi.mock('@/plugins/router', () => ({ replacePath: vi.fn() }))
vi.mock('@/hooks/search', () => ({ useSearch: () => ({ parseQ: vi.fn() }) }))
vi.mock('@/hooks/tags', () => ({ useTags: () => ({ fetch: vi.fn(), tags: ref([]) }) }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { subscribe: () => vi.fn() } }))
vi.mock('@/hooks/show-action-error', () => ({ showActionError: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/lib/api/mutation', () => ({
  deleteContactsGQL: 'deleteContacts',
  initMutation: () => ({ mutate: state.mutate, loading: ref(false), onDone: vi.fn() }),
}))
vi.mock('@/lib/api/query', () => ({
  contactsGQL: 'contacts',
  contactSourcesGQL: 'contactSources',
  initQuery: vi.fn(),
  initLazyQuery: ({ handle }: { handle: (data: unknown, error: string) => void }) => ({
    loading: ref(false),
    fetch: () => handle({ contacts: state.rows, contactCount: state.rows.length }, ''),
  }),
}))

it('does not delete a contact removed by a same-route tag refresh', async () => {
  vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() })
  let contacts!: ReturnType<typeof useContactsData>
  const Page = { setup() { contacts = useContactsData(); return () => h('section') } }
  const view = renderView({ setup: () => () => h(KeepAlive, null, () => h(Page)) })
  try {
    contacts.toggleSelect({ shiftKey: false } as MouseEvent, contacts.items.value[2]!, 2)
    contacts.deleteItems()
    expect(contacts.confirmingDelete.value).toBe(true)

    state.rows = state.rows.slice(0, 2)
    emitter.emit('item_tags_updated', { type: DataType.CONTACT })
    expect(contacts.items.value.map(({ id }) => id)).toEqual(['a', 'b'])
    expect(contacts.selectedIds.value).toEqual([])
    expect(contacts.confirmingDelete.value).toBe(false)
    await contacts.doDeleteItems()
    expect(state.mutate).not.toHaveBeenCalled()

    contacts.toggleSelect({ shiftKey: true } as MouseEvent, contacts.items.value[1]!, 1)
    expect(contacts.selectedIds.value).toEqual(['b'])
    contacts.deleteItems()
    await contacts.doDeleteItems()
    expect(state.mutate).toHaveBeenCalledExactlyOnceWith({ query: 'ids:b' })
  } finally {
    view.unmount()
    vi.unstubAllGlobals()
  }
})
