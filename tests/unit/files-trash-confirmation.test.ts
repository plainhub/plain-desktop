import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { h, KeepAlive, provide, ref } from 'vue'
import FilesTrashView from '@/views/files/FilesTrashView.vue'
import FilePreviewListItem from '@/views/files/FilePreviewListItem.vue'
import FileTrashActions from '@/views/files/FileTrashActions.vue'
import FileItemDetails from '@/views/files/FileItemDetails.vue'
import ListItemPhone from '@/components/base/ListItemPhone.vue'
import FileItemLayout from '@/views/files/FileItemLayout.vue'
import { click, clientTemplate, fire, flush, nodes, renderView } from '../helpers/vue-node-renderer'

const state = vi.hoisted(() => ({
  rows: [] as { path: string; name: string; size: number; updatedAt: string; isDir: boolean }[],
  preview: vi.fn(),
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
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ urlTokenKey: ref(null) }) }))
vi.mock('pinia', () => ({ storeToRefs: (value: unknown) => value }))
vi.mock('@/components/OnlinePreviewIcon.vue', () => ({ default: { render: () => null } }))
vi.mock('@/lib/api/text-file', () => ({ getTextFileId: (_key: unknown, path: string) => path }))
vi.mock('@/lib/browser', () => ({ openUrl: state.preview }))
vi.mock('@/hooks/files', () => ({ useDownload: () => ({ downloadFile: vi.fn() }), useView: () => ({ view: vi.fn() }) }))
vi.mock('@/hooks/open-media', () => ({ useOpenMedia: () => ({ open: vi.fn() }) }))

clientTemplate(FilesTrashView, 'src/views/files/FilesTrashView.vue')
clientTemplate(FilePreviewListItem, 'src/views/files/FilePreviewListItem.vue')
clientTemplate(FileTrashActions, 'src/views/files/FileTrashActions.vue')
clientTemplate(FileItemDetails, 'src/views/files/FileItemDetails.vue')
clientTemplate(ListItemPhone, 'src/components/base/ListItemPhone.vue')
clientTemplate(FileItemLayout, 'src/views/files/FileItemLayout.vue')

beforeEach(() => {
  vi.stubGlobal('__IS_TAURI__', false)
  vi.stubGlobal('localStorage', { getItem: () => null })
  vi.stubGlobal('window', { getSelection: () => null, addEventListener: vi.fn(), removeEventListener: vi.fn() })
  state.rows = ['a', 'b'].map(key => ({ path: `/disk/.plain-trash/files/f-123-${key}.txt`, name: `${key}.txt`, size: 12, updatedAt: '2026-11-22T00:00:00Z', isDir: false }))
  state.purge.mockClear()
  state.preview.mockClear()
})

afterEach(() => vi.unstubAllGlobals())

function mountTrash(isPhone = false) {
  return renderView({ setup() {
    provide('isPhone', ref(isPhone))
    return () => h(KeepAlive, null, () => h(FilesTrashView))
  } }, {}, {
    NoDataPlaceholder: () => null,
    FileThumb: { render: () => h('span') },
    OnlinePreviewIcon: () => null,
    ListItemPhone,
    VDropdown: () => null,


    FieldId: { render: () => h('span') },
    VCheckbox: { render: () => h('input') },
    VIconButton: { render: () => h('button') },
    DeleteConfirmPopup: { render: () => h('div', { 'data-popup': true }) },
  })
}

it('requires a new permanent-delete confirmation if selected Trash paths disappear', async () => {
  const view = mountTrash()
  const element = (key: string, value: unknown) => nodes(view.root).find((node) => node.props[key] === value)!
  const openBulk = async () => {
    fire(element('id', 'files-trash-bulk'), 'onClick', {})
    await flush()
  }
  try {
    await flush()
    fire(nodes(view.root).find((node) => node.tag === 'input')!, 'onChange', { target: { checked: true } })
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
    expect(state.purge).toHaveBeenCalledExactlyOnceWith({ paths: ['/disk/.plain-trash/files/f-123-b.txt'] })
  } finally { view.unmount() }
})


it.each([false, true])('opens a trashed file and reuses row selection (phone: %s)', async (isPhone) => {
  const view = mountTrash(isPhone)
  try {
    await flush()
    const rows = () => nodes(view.root).filter(node => String(node.props.class).split(' ').includes(isPhone ? 'list-item-phone' : 'file-item'))
    click(rows()[0]!)
    await flush()
    expect(state.preview).toHaveBeenCalledExactlyOnceWith('/text-file?id=' + encodeURIComponent('/disk/.plain-trash/files/f-123-a.txt'))
    expect(String(rows()[0]?.props.class)).not.toContain(' selected')

    click(nodes(rows()[0]!).find(node => node.tag === 'input')!)
    await flush()
    click(rows()[1]!)
    await flush()
    expect(rows().every(row => String(row.props.class).includes(' selected'))).toBe(true)
    expect(state.preview).toHaveBeenCalledTimes(1)
  } finally {
    view.unmount()
  }
})
