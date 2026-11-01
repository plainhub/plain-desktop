import { afterEach, expect, it, vi } from 'vitest'
import { createSSRApp, defineComponent, h } from 'vue'
import { renderToString } from 'vue/server-renderer'

const state = vi.hoisted(() => ({ data: undefined as any, events: undefined as any }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('pinia', () => ({ storeToRefs: (value: unknown) => value }))
vi.mock('@/stores/temp', async () => {
  const { ref } = await import('vue')
  return { useTempStore: () => ({ app: ref({ permissions: ['WRITE_EXTERNAL_STORAGE'] }), urlTokenKey: ref(null), uploads: ref([]) }) }
})
vi.mock('@/stores/files', async () => {
  const { ref } = await import('vue')
  return { useFilesStore: () => ({ selectedFiles: ref([]), isCut: ref(false) }) }
})
vi.mock('@/hooks/device', async () => { const { ref } = await import('vue'); return { useVirtualRowCount: () => ref(18) } })
vi.mock('@/hooks/files', async () => {
  const { ref } = await import('vue')
  const fn = vi.fn
  return {
    useCreateDir: () => ({ createPath: ref(''), createVariables: fn(), createMutation: fn() }),
    useRename: () => ({ renameItem: ref(), renameDone: fn(), renameMutation: fn(), renameVariables: fn() }),
    useMounts: () => ({ mounts: ref([]), refetch: fn() }),
    useDownload: () => ({ downloadFile: fn(), downloadFileAs: fn(), downloadDir: fn(), downloadFiles: fn() }),
    useView: () => ({ view: fn() }),
    useCopyPaste: () => ({ loading: ref(false), canPaste: () => false, copy: fn(), cut: fn(), paste: fn() }),
  }
})
vi.mock('@/hooks/upload', async () => {
  const { ref } = await import('vue')
  return {
    useDragDropUpload: () => ({ dropping: ref(false), fileDragEnter: vi.fn(), fileDragLeave: vi.fn(), dropFiles: vi.fn() }),
    useFileUpload: () => ({ input: ref(), upload: vi.fn(), uploadChanged: vi.fn() }),
  }
})
vi.mock('@/hooks/key-events', () => ({ useFilesKeyEvents: () => ({ keyDown: vi.fn(), keyUp: vi.fn() }) }))
vi.mock('@/hooks/open-media', () => ({ useOpenMedia: () => ({ open: vi.fn() }) }))
vi.mock('@/components/modal', () => ({ openModal: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/components/EditValueModal.vue', () => ({ default: {} }))
vi.mock('@/components/KeyboardShortcutsModal.vue', () => ({ default: {} }))
vi.mock('@/components/NoDataPlaceholder.vue', () => ({ default: { render: () => null } }))
vi.mock('@/lib/api/mutation', async () => {
  const { ref } = await import('vue')
  return { initMutation: () => ({ loading: ref(false), mutate: vi.fn(), onDone: vi.fn() }),
    setTempValueGQL: '', addFavoriteFolderGQL: '', deleteFilesGQL: '' }
})
vi.mock('@/views/files/hooks/useFilesDelete', async () => {
  const { ref } = await import('vue')
  return { useFilesDelete: () => ({ deletingIds: ref([]), collapsingIds: ref([]), bulkDeleting: ref(false), isDeleting: () => false, startDelete: vi.fn() }) }
})
vi.mock('@/views/files/hooks/useFilesData', async () => {
  const { ref, reactive } = await import('vue')
  return { useFilesData: () => {
    if (state.data) return state.data
    state.data = {
      filter: reactive({ parent: '/', rootPath: '/', text: '', showHidden: false }), q: ref(''),
      items: ref([
        { id: '/usr', path: '/usr', name: 'usr', isSystem: true },
        { id: '/docs', path: '/docs', name: 'docs', isSystem: false },
        { id: '/data', path: '/data', name: 'data', isSystem: false },
      ]), totalRef: ref(3), loading: ref(false), firstInit: ref(false), refreshing: ref(false), sorting: ref(false),
      isActive: ref(true), rootDir: ref('/'), indexed: ref(false), fileSortBy: ref('NAME_ASC'),
      buildQ: vi.fn(), applyRouteQuery: vi.fn(), fetch: vi.fn(), route: { path: '/files' }, cancel: vi.fn(), searchStatus: ref('idle'),
    }
    return state.data
  } }
})
vi.mock('@/views/files/hooks/useFilesBreadcrumb', () => ({ useFilesBreadcrumb: () => ({ breadcrumbPaths: [{ path: '/', name: 'Storage' }], getPageStats: () => '' }) }))
vi.mock('@/views/files/hooks/useFilesNavigation', () => ({ useFilesNavigation: () => ({ navigateToDir: vi.fn(), toggleShowHidden: vi.fn(), clickItem: vi.fn(), viewItem: vi.fn() }) }))
vi.mock('@/views/files/hooks/useFilesEvents', () => ({ useFilesEvents: (options: unknown) => { state.events = options } }))
vi.mock('@/views/files/hooks/useFileIndexStatus', async () => {
  const { ref } = await import('vue')
  return { useFileIndexStatus: () => ({ progress: ref(), error: ref('') }) }
})
vi.mock('@/components/virtualscroll', async () => {
  const { defineComponent, h } = await import('vue')
  return { default: defineComponent({ props: ['dataSources'], setup: (props, { slots }) => () => h('div', [slots.header?.(), ...props.dataSources.map((item: unknown, index: number) => slots.item?.({ item, index }))]) }) }
})
vi.mock('@/views/files/SystemFilesHeader.vue', async () => {
  const { defineComponent, h } = await import('vue')
  return { default: defineComponent({ props: ['count', 'expanded'], emits: ['toggle'], setup: (props, { emit }) => () => h('button', { id: 'systems', onClick: () => emit('toggle') }, `systems:${props.count}`) }) }
})
import FilesView from '@/views/files/FilesView.vue'

afterEach(() => { state.data = undefined; state.events = undefined })
async function render() {
  const app = createSSRApp(FilesView)
  app.provide('isPhone', false)
  app.config.globalProperties.$t = (key: string, value?: any) => key === 'x_selected' ? `${value.count} selected` : key
  app.directive('tooltip', {})
  for (const name of ['FilesActionButtons', 'BulkDeleteButton', 'BulkDownloadButton', 'VIconButton', 'FileSkeletonItem']) app.component(name, { render: () => null })
  app.component('VCheckbox', defineComponent({ props: ['checked'], emits: ['change'], setup: props => () => h('input', { type: 'checkbox', checked: props.checked }) }))
  app.component('FileListItem', defineComponent({ props: ['item'], setup: props => () => h('span', { file: props.item.id }, props.item.name) }))
  return renderToString(app)
}

it('keeps the ordinary count correct after deleting ordinary and system entries', async () => {
  expect(await render()).toContain('Storage (2)')
  state.events.onDeleted([state.data.items.value[1]])
  expect(state.data.totalRef.value).toBe(2)
  expect(await render()).toContain('Storage (1)')
  state.events.onDeleted([state.data.items.value[0]])
  const html = await render()
  expect(html).toContain('Storage (1)')
  expect(html).not.toContain('systems:')
  expect(html).toContain('file="/data"')
})

it('preserves the full result count when only part of the directory is loaded', async () => {
  await render()
  state.data.totalRef.value = 10003
  state.events.onDeleted([state.data.items.value[1]])
  expect(state.data.totalRef.value).toBe(10002)
  expect(await render()).toContain('Storage (10001)')
  state.events.onDeleted([{ id: '/unrelated', path: '/unrelated' }])
  expect(state.data.totalRef.value).toBe(10002)
})
