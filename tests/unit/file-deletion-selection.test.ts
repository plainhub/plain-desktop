import { afterEach, expect, it, vi } from 'vitest'
import { computed, createRenderer, h, KeepAlive, ref } from 'vue'
import type { IFile } from '@/lib/file'
import emitter from '@/plugins/eventbus'

const state = vi.hoisted(() => ({ loadRecent: (_: { recentFiles: IFile[] }, _error: string) => {}, remove: vi.fn() }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { subscribe: () => vi.fn() } }))
vi.mock('@/lib/api/query', () => ({
  recentFilesGQL: 'recent',
  initLazyQuery: ({ handle }: { handle: typeof state.loadRecent }) => {
    state.loadRecent = handle
    return { loading: ref(false), fetch: vi.fn() }
  },
}))
vi.mock('@/lib/api/mutation', () => ({
  setTempValueGQL: 'setTempValue', addFavoriteFolderGQL: 'favorite', trashFilesGQL: 'delete',
  initMutation: (options: { document: string }) => ({
    mutate: options.document === 'delete' ? state.remove : vi.fn(), onDone: vi.fn(), loading: ref(false),
  }),
}))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({}), urlTokenKey: ref(null) }) }))
vi.mock('@/lib/file', () => ({ enrichFile: (item: IFile) => item, fileNameError: vi.fn() }))
vi.mock('@/lib/api/file', () => ({ getFileUrlByPath: vi.fn() }))
vi.mock('@/lib/api/text-file', () => ({ getTextFileId: vi.fn() }))
vi.mock('@/lib/browser', () => ({ openUrl: vi.fn() }))
vi.mock('@/lib/clipboard', () => ({ copyTextToClipboard: vi.fn() }))
vi.mock('@/components/modal', () => ({ openModal: vi.fn() }))
vi.mock('@/components/EditValueModal.vue', () => ({ default: {} }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/hooks/files', () => ({
  useDownload: () => ({ downloadFile: vi.fn(), downloadFiles: vi.fn(), downloadDir: vi.fn() }),
  useView: () => ({ view: vi.fn() }),
}))
vi.mock('@/hooks/open-media', () => ({ useOpenMedia: () => ({ open: vi.fn() }) }))
vi.mock('@/hooks/device', () => ({ getIsPhone: () => ref(false) }))
vi.mock('@/hooks/file-open', () => ({ useFileOpen: () => ({ openFile: vi.fn() }) }))

import { useFilesRecent } from '@/hooks/files-recent'
import { useSelectable } from '@/hooks/list'
import { useFilesActions } from '@/views/files/hooks/useFilesActions'
import { useFilesEvents } from '@/views/files/hooks/useFilesEvents'

const file = (name: string) => ({ id: `/fixtures/${name}`, path: `/fixtures/${name}`, name }) as IFile
const first = file('first.txt')
const second = file('second.txt')
const click = { shiftKey: false } as MouseEvent

function mountPage(setup: () => void) {
  vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() })
  const renderer = createRenderer<object, object>({
    insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
    setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
  })
  const Page = { setup() { setup(); return () => null } }
  const app = renderer.createApp({ render: () => h(KeepAlive, null, () => h(Page)) })
  app.mount({})
  return () => app.unmount()
}

afterEach(() => vi.unstubAllGlobals())

it('keeps a surviving selection in Recent Files after a file-viewer deletion', () => {
  let page!: ReturnType<typeof useFilesRecent>
  const unmount = mountPage(() => { page = useFilesRecent() })
  try {
    state.loadRecent({ recentFiles: [first, second] }, '')
    page.toggleSelect(click, first, 0)
    page.toggleSelect(click, second, 1)
    emitter.emit('file_deleted', { item: first })
    expect(page.selectedIds.value).toEqual([second.id])
    expect(page.checked.value).toBe(true)
  } finally { unmount() }
})

it('keeps a surviving selection in Files and cancels a confirmation for the removed file', () => {
  const items = ref([first, second])
  const selection = useSelectable(items)
  let actions!: ReturnType<typeof useFilesActions>
  const noop = () => {}
  const unmount = mountPage(() => {
    actions = useFilesActions({
      items, total: ref(2), selectedIds: selection.selectedIds, clearSelection: selection.clearSelection,
      visibleItems: computed(() => items.value), retainSelection: selection.retainSelection,
      filter: { parent: '/fixtures' } as Parameters<typeof useFilesActions>[0]['filter'],
      rootDir: computed(() => '/fixtures'), urlTokenKey: ref(null), t: (key) => key,
      fetch: noop, refetchMounts: noop, copy: noop, cut: noop, paste: noop,
      downloadFile: noop, downloadDir: noop, downloadFiles: noop,
      createPath: ref(''), createVariables: noop, createMutation: noop,
      renameItem: ref(), renameMutation: noop, renameVariables: noop, renameDone: () => true,
    })
    useFilesEvents({
      isActive: ref(false), fileSortBy: ref(''), routeFullPath: () => '/files', applyRouteQuery: noop,
      fetch: noop, refetchMounts: noop, refreshing: ref(false), sorting: ref(false),
      onDeleted: actions.onDeleted, pageKeyDown: noop, pageKeyUp: noop, parentDir: () => '/fixtures',
    })
  })
  try {
    selection.toggleSelect(click, first, 0)
    selection.toggleSelect(click, second, 1)
    actions.trashItems()
    expect(actions.confirmingTrash.value).toBe(true)
    emitter.emit('file_deleted', { item: first })
    expect(selection.selectedIds.value).toEqual([second.id])
    expect(actions.confirmingTrash.value).toBe(false)
  } finally { unmount() }
})
