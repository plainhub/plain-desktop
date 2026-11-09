import { afterEach, expect, it, vi } from 'vitest'
import * as Vue from 'vue'
import { createRenderer, h, ref, ssrContextKey } from 'vue'
import { compileScript, compileTemplate, parse } from 'vue/compiler-sfc'
import { DataType } from '@/lib/data'
import type { IApp, IDoc } from '@/lib/interfaces'
import { openWindow } from '@/lib/api/tauri-window'
import { setLocalServerToken } from '@/lib/api/api'
import { openUrl } from '@/lib/browser'
import { openUrl as openExternalUrl } from '@/lib/tauri-plugins'
import OnlinePreviewIcon from '@/components/OnlinePreviewIcon.vue'
import previewIconSource from '@/components/OnlinePreviewIcon.vue?raw'
import DocListItem from '@/views/docs/DocListItem.vue'
import docListItemSource from '@/views/docs/DocListItem.vue?raw'
import { useDocsActions } from '@/views/docs/hooks/useDocsActions'

const downloadFile = vi.hoisted(() => vi.fn())
vi.mock('@/hooks/files', () => ({ useDownload: () => ({ downloadFile }) }))
vi.mock('@/lib/api/tauri-window', () => ({ openWindow: vi.fn() }))
vi.mock('@/lib/device/client-id', () => ({ getDesktopClientId: () => 'fixture-desktop' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => true }))
vi.mock('@/lib/tauri-plugins', () => ({ openUrl: vi.fn() }))
vi.mock('@/lib/format', () => ({
  formatFileSize: String,
  formatDateTimeFull: String,
  formatTimeAgo: String,
}))
vi.mock('@/lib/api/file', () => ({
  getFileName: (path: string) => path.split('/').pop(),
  tokenToKey: (token: string) => token,
  getFileId: (key: unknown, path: string, _: string, scope: { mode: string; id: string }) => {
    expect(key).toBe('native-session')
    return `${scope.mode}.${scope.id}.${path}`
  },
  getFileUrlByPath: (_: unknown, path: string) => 'http://127.0.0.1:8080/fs?id=' + encodeURIComponent(path),
}))
vi.mock('@/lib/api/mutation', () => ({
  copyFileGQL: '',
  renameFileGQL: '',
  initMutation: () => ({ mutate: vi.fn(), onDone: vi.fn() }),
}))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/modal', () => ({ openModal: vi.fn() }))
vi.mock('@/components/EditValueModal.vue', () => ({ default: {} }))
vi.mock('@/components/DeleteFileConfirm.vue', () => ({ default: {} }))
vi.mock('@/views/docs/DocActionButtons.vue', () => ({ default: { render: () => null } }))

for (const [component, source, filename] of [
  [DocListItem, docListItemSource, 'DocListItem.vue'],
  [OnlinePreviewIcon, previewIconSource, 'OnlinePreviewIcon.vue'],
] as const) {
  const { descriptor } = parse(source, { filename })
  const template = compileTemplate({
    source: descriptor.template!.content,
    filename,
    id: filename,
    compilerOptions: {
      mode: 'function',
      prefixIdentifiers: true,
      bindingMetadata: compileScript(descriptor, { id: filename }).bindings,
    },
  })
  component.render = new Function('Vue', template.code)(Vue)
}

setLocalServerToken('native-session')

afterEach(() => vi.unstubAllGlobals())

it('opens the same preview route in a browser tab on the Web', async () => {
  vi.clearAllMocks()
  const popup = vi.fn()
  vi.stubGlobal('__IS_TAURI__', false)
  vi.stubGlobal('window', { open: popup })
  await openUrl('/text-file?id=fixture')
  expect(popup).toHaveBeenCalledExactlyOnceWith('/text-file?id=fixture', '_blank')
  expect(openWindow).not.toHaveBeenCalled()
})

it.each([
  ['Notes', 'md', 'text'],
  ['Quarterly report', 'DOC', 'text'],
  ['Manual', 'pdf', 'browser'],
  ['Configuration.md', 'toml', 'download'],
] as const)('uses the actual file name for the marker and native row click on %s (%s)', async (title, extension, action) => {
  vi.clearAllMocks()
  const popup = vi.fn()
  vi.stubGlobal('__IS_TAURI__', true)
  vi.stubGlobal('window', { open: popup })
  const path = `/fixtures/${title}.${extension}`
  const item: IDoc = {
    id: 'fixture', title, path, extension, size: 42, bucketId: '', tags: [],
    createdAt: '2026-01-01T00:00:00Z', updatedAt: '2026-01-01T00:00:00Z',
  }
  let hasMarker = false
  let clickRow: ((event: MouseEvent) => void) | undefined
  const renderer = createRenderer<object, { tag: string }>({
    insert: vi.fn(),
    remove: vi.fn(),
    createElement: (tag: string) => ({ tag }),
    createText: () => ({}),
    createComment: () => ({}),
    setText: vi.fn(),
    setElementText: vi.fn(),
    parentNode: () => null,
    nextSibling: () => null,
    patchProp: (node, key, _, value) => {
      if (key === 'class' && value === 'online-preview-icon') hasMarker = true
      if (node.tag === 'section' && key === 'onClick') clickRow = value
    },
  })
  const app = renderer.createApp({
    setup() {
      const { openFile } = useDocsActions(ref([item]), ref([]), vi.fn(), vi.fn(), ref(null))
      return () => h(DocListItem, {
        item, index: 0, selectedIds: [], shiftEffectingIds: [], shouldSelect: false,
        filter: { tagIds: [] }, app: {} as IApp, dataType: DataType.DOC, tags: [],
        trashLoading: () => false, restoreLoading: () => false, trash: vi.fn(), restore: vi.fn(),
        deleteItem: vi.fn(), bucketsMap: {}, viewBucket: vi.fn(), handleMouseOver: vi.fn(),
        handleItemClick: (_event, _item, _index, open) => open(),
        toggleSelect: vi.fn(), onOpenFile: openFile,
      })
    },
  })
  app.config.globalProperties.$t = (key: string) => key
  app.provide(ssrContextKey, { modules: new Set() })
  app.directive('tooltip', {})
  for (const component of ['VCheckbox', 'FieldId', 'ItemTags']) {
    app.component(component, { render: () => null })
  }
  app.mount({ tag: 'root' })
  try {
    expect.soft(hasMarker).toBe(action !== 'download')
    expect(clickRow).toBeDefined()
    clickRow!({ stopPropagation() {} } as MouseEvent)
    await vi.dynamicImportSettled()
    expect.soft(popup).not.toHaveBeenCalled()
    if (action === 'download') {
      expect(downloadFile).toHaveBeenCalledExactlyOnceWith(path, title)
      expect(openWindow).not.toHaveBeenCalled()
      expect(openExternalUrl).not.toHaveBeenCalled()
    } else if (action === 'text') {
      expect(openWindow).toHaveBeenCalledExactlyOnceWith(`/text-file?id=${encodeURIComponent(`c.fixture-desktop.${path}`)}`)
      expect(openExternalUrl).not.toHaveBeenCalled()
      expect(downloadFile).not.toHaveBeenCalled()
    } else {
      expect(openExternalUrl).toHaveBeenCalledExactlyOnceWith(`http://127.0.0.1:8080/fs?id=${encodeURIComponent(path)}`)
      expect(openWindow).not.toHaveBeenCalled()
      expect(downloadFile).not.toHaveBeenCalled()
    }
  } finally {
    app.unmount()
  }
})
