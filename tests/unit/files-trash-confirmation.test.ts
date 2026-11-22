import { expect, it, vi } from 'vitest'
import { h, KeepAlive, provide, ref } from 'vue'
import FilesTrashView from '@/views/files/FilesTrashView.vue'
import { clientTemplate, fire, flush, nodes, renderView } from '../helpers/vue-node-renderer'

const state = vi.hoisted(() => ({
  rows: [{ path: '/trash/a' }, { path: '/trash/b' }],
  purge: vi.fn(),
  refresh: () => {},
}))

vi.mock('@/lib/api/query', () => ({ trashedFilesGQL: 'list' }))
vi.mock('@/lib/api/mutation', () => ({ deleteTrashedFilesGQL: 'purge', restoreFilesGQL: 'restore' }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: async (operation: string, parameters: unknown) => {
  if (operation === 'purge') state.purge(parameters)
  return { data: { trashedFiles: state.rows } }
} }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe: (event: string, refresh: () => void) => {
    if (event === 'contentChanged') state.refresh = refresh
    return () => {}
  },
  onAttached: () => () => {},
} }))
vi.mock('@/hooks/show-action-error', () => ({ showActionError: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/views/files/FileTrashItem.vue', () => ({ default: { render: () => h('article') } }))

clientTemplate(FilesTrashView, 'src/views/files/FilesTrashView.vue')

it('requires a new permanent-delete confirmation if selected Trash paths disappear', async () => {
  const view = renderView({ setup() {
    provide('isPhone', ref(false))
    return () => h(KeepAlive, null, () => h(FilesTrashView))
  } }, {}, {
    NoDataPlaceholder: () => null,
    VCheckbox: { render: () => h('input') },
    VIconButton: { render: () => h('button') },
    DeleteConfirmPopup: { render: () => h('div', { 'data-popup': true }) },
  })
  const element = (key: string, value: unknown) => nodes(view.root).find((node) => node.props[key] === value)!
  const openBulk = async () => {
    fire(element('id', 'files-trash-bulk'), 'onClick', {})
    await flush()
  }
  try {
    await flush()
    fire(nodes(view.root).find((node) => node.tag === 'input')!, 'onChange', {})
    await flush()
    await openBulk()
    const original = element('data-popup', true)
    expect(original.props['model-value']).toBe(true)
    expect(original.props.count).toBe(2)
    const staleConfirm = original.props.onConfirm

    state.rows = [state.rows[1]]
    state.refresh()
    await flush()
    expect(element('data-popup', true).props['model-value']).toBe(false)
    await staleConfirm()
    expect(state.purge).not.toHaveBeenCalled()

    await openBulk()
    const next = element('data-popup', true)
    expect(next.props['model-value']).toBe(true)
    expect(next.props.count).toBe(1)
    fire(next, 'onConfirm', {})
    expect(state.purge).toHaveBeenCalledExactlyOnceWith({ paths: ['/trash/b'] })
  } finally { view.unmount() }
})
