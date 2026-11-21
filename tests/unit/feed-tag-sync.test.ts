import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { defineComponent, h, KeepAlive, provide, ref } from 'vue'
import TagFilter from '@/components/TagFilter.vue'
import TagRelationsDropdown from '@/components/TagRelationsDropdown.vue'
import { useFeedEntry } from '@/hooks/feed-entry'
import emitter from '@/plugins/eventbus'
import type { ITag } from '@/lib/interfaces'
import { clientTemplate, click, content, flush, nodes, renderView } from '../helpers/vue-node-renderer'

const backend = vi.hoisted(() => ({ tags: [] as ITag[], queries: vi.fn(), modal: null as any }))
vi.mock('vue-router', () => ({ useRoute: () => ({ params: { id: 'entry' }, query: {} }) }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({}), urlTokenKey: ref(null) }) }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('@/hooks/markdown', () => ({ useSafeMarkdown: () => ({ render: async (text: string) => text }) }))
vi.mock('@/hooks/feeds', () => ({ useFeeds: () => ({ viewFeed: vi.fn() }) }))
vi.mock('@/plugins/router', () => ({ replacePath: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/components/modal', () => ({ openModal: (_component: unknown, props: unknown) => { backend.modal = props } }))
vi.mock('@/components/EditValueModal.vue', () => ({ default: {} }))
vi.mock('@/lib/api/query', () => ({
  feedEntryGQL: 'entry', tagsGQL: 'tags',
  initQuery: ({ handle, variables }: any) => {
    const refetch = () => {
      backend.queries(variables.type)
      handle({ tags: backend.tags.map(tag => ({ ...tag })) }, '')
    }
    refetch()
    return { refetch }
  },
  initLazyQuery: () => ({ loading: ref(false), fetch: vi.fn() }),
}))
vi.mock('@/lib/api/mutation', () => ({
  createTagGQL: 'create', updateTagGQL: 'rename', deleteTagGQL: 'delete', updateTagRelationsGQL: 'relations',
  saveFeedEntriesToNotesGQL: 'notes', syncFeedEntryContentGQL: 'sync',
  initMutation: ({ document }: { document: string }) => {
    const callbacks: Array<(data: any) => void> = []
    return {
      loading: ref(false), onDone: (callback: (data: any) => void) => callbacks.push(callback),
      mutate: ({ name, id }: any) => {
        if (document === 'create') backend.tags.push({ id: 'new-tag', name, count: 0 } as ITag)
        if (document === 'rename') backend.tags.find(tag => tag.id === id)!.name = name
        if (document === 'delete') backend.tags = backend.tags.filter(tag => tag.id !== id)
        callbacks.forEach(callback => callback({ data: {} }))
        return Promise.resolve()
      },
    }
  },
}))

clientTemplate(TagFilter, 'src/components/TagFilter.vue')
clientTemplate(TagRelationsDropdown, 'src/components/TagRelationsDropdown.vue')
const cleanup: Array<() => void> = []
beforeEach(() => { backend.tags = []; backend.queries.mockClear(); backend.modal = null })
afterEach(() => { cleanup.splice(0).forEach(unmount => unmount()); vi.unstubAllGlobals() })

function mount() {
  vi.stubGlobal('document', { getElementById: vi.fn(), dispatchEvent: vi.fn() })
  const active = ref(true)
  const Reader = defineComponent({
    setup() {
      const reader = useFeedEntry()
      return () => h(TagRelationsDropdown, { type: 'FEED_ENTRY', tags: reader.tags.value, item: { key: 'entry' }, selected: [] })
    },
  })
  const view = renderView({ setup() {
    provide('isPhone', false)
    return () => h('main', [
    h(TagFilter, { type: 'FEED_ENTRY', selected: '' }),
    h(KeepAlive, null, { default: () => active.value ? h(Reader) : h('aside') }),
  ]) } }, {}, {
    'v-icon-button': { setup: (_: unknown, { slots }: any) => () => h('button', slots.default?.()) },
    'v-dropdown': { setup: (_: unknown, { slots }: any) => () => h('section', [slots.trigger?.(), slots.default?.()]) },
    'v-dropdown-menu': { props: ['modelValue'], setup: (props: any, { slots }: any) => () => props.modelValue ? h('section', slots.default?.()) : null },
    'v-checkbox': { render: () => h('input') },
    SidebarListItem: { props: ['title'], setup: (props: any, { slots }: any) => () => h('li', [props.title, slots.actions?.()]) },
    'delete-confirm-popup': {
      inheritAttrs: false, props: ['modelValue', 'confirming'], emits: ['confirm', 'cancel'],
      setup: (props: any, { emit, slots }: any) => () => props.modelValue
        ? (props.confirming ? h('button', { onClick: () => emit('confirm') }, 'confirm-delete') : slots.menu?.())
        : null,
    },
  })
  cleanup.push(view.unmount)
  const picker = () => nodes(view.root).find(node => node.props.class === 'tag-picker-list')!
  const add = async () => {
    click(nodes(view.root).find(node => node.tag === 'button')!)
    const mutation = backend.modal.mutation()
    await mutation.mutate(backend.modal.getVariables('Fresh tag'))
    await flush()
  }
  return { ...view, active, picker, add }
}

it('shows a sidebar-created tag in the already mounted article picker', async () => {
  const view = mount()
  expect(content(view.picker())).toContain('no_data')
  await view.add()
  expect(content(view.picker())).toContain('Fresh tag')
})

it('updates picker names and removes deleted sidebar tags', async () => {
  backend.tags = [{ id: 'new-tag', name: 'Original tag', count: 0 } as ITag]
  const view = mount()
  click(nodes(view.root).find(node => node.props.id === 'tag-new-tag')!)
  await flush()
  click(nodes(view.root).find(node => node.props.class === 'dropdown-item' && content(node) === 'rename')!)
  await backend.modal.mutation().mutate(backend.modal.getVariables('Renamed tag'))
  backend.modal.done()
  await flush()
  expect(content(view.picker())).toContain('Renamed tag')
  expect(content(view.picker())).not.toContain('Original tag')
  click(nodes(view.root).find(node => node.props.id === 'tag-new-tag')!)
  await flush()
  click(nodes(view.root).find(node => node.props.class === 'dropdown-item' && content(node) === 'delete')!)
  await flush()
  click(nodes(view.root).find(node => node.tag === 'button' && content(node) === 'confirm-delete')!)
  await flush()
  expect(content(view.picker())).toContain('no_data')
})

it('filters refreshes by type and resynchronizes after a cached article returns', async () => {
  const view = mount()
  const requests = backend.queries.mock.calls.length
  emitter.emit('refetch_tags', 'NOTE')
  expect(backend.queries).toHaveBeenCalledTimes(requests)
  view.active.value = false
  await flush()
  await view.add()
  expect(backend.queries.mock.calls.length - requests).toBe(1)
  view.active.value = true
  await flush()
  expect(content(view.picker())).toContain('Fresh tag')
  view.unmount()
  cleanup.pop()
  backend.queries.mockClear()
  emitter.emit('refetch_tags', 'FEED_ENTRY')
  expect(backend.queries).not.toHaveBeenCalled()
})
