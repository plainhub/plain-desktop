import { expect, it, vi } from 'vitest'
import { h, ref } from 'vue'
import DatabaseView from '@/views/developer/DeveloperDatabaseView.vue'
import DevDataTable from '@/views/developer/DevDataTable.vue'
import { click, clientTemplate, flush, nodes, renderView } from '../helpers/vue-node-renderer'

const { mutate, columns, row } = vi.hoisted(() => ({
  mutate: vi.fn(),
  columns: [
    { name: 'title', primaryKey: false }, { name: 'root_path', primaryKey: true },
    { name: 'relative_path', primaryKey: true },
  ],
  row: { title: 'Second', root_path: '/root', relative_path: 'two' },
}))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({ developerMode: true }) }) }))
vi.mock('@/hooks/show-action-error', () => ({ showActionError: vi.fn() }))
vi.mock('@/lib/api/mutation', () => ({ deleteDbTableRowsGQL: 'delete', initMutation: () => ({ mutate }) }))
vi.mock('@/views/developer/DeveloperDatabaseSidebar.vue', () => ({ default: { render: () => null } }))
vi.mock('@/lib/api/query', () => ({
  dbPathGQL: 'path', dbTablesGQL: 'tables', dbTableRowCountGQL: 'count', dbTableRowsGQL: 'rows',
  initQuery: ({ document, handle }: any) => {
    if (document === 'tables') queueMicrotask(() => handle({ dbTables: [{ name: 'favorite_folders', columns }] }, ''))
    return { loading: ref(false), refetch: vi.fn() }
  },
  initLazyQuery: ({ document, handle }: any) => ({
    loading: ref(false),
    fetch: () => handle(document === 'rows' ? { dbTableRows: [row] } : { dbTableRowCount: 1 }, ''),
  }),
}))

clientTemplate(DevDataTable, 'src/views/developer/DevDataTable.vue')
clientTemplate(DatabaseView, 'src/views/developer/DeveloperDatabaseView.vue')

it('sends every primary key column of the clicked row and omits display columns', async () => {
  const empty = { render: () => null }
  const view = renderView(DatabaseView, {}, {
    VIconButton: { render: () => h('button') }, FieldId: empty, VPagination: empty,
    VCircularProgress: empty, VDropdown: empty,
  })
  try {
    await flush()
    click(nodes(view.root).find(n => n.tag === 'button' && n.props.class === 'delete-btn')!)
    await flush()
    expect(mutate).toHaveBeenCalledExactlyOnceWith({
      table: 'favorite_folders', keys: [{ root_path: '/root', relative_path: 'two' }],
    })
  } finally { view.unmount() }
})
