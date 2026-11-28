import { afterEach, expect, it, vi } from 'vitest'
import { defineComponent, h, provide, ref, type FunctionalComponent } from 'vue'
import { createPinia, defineStore, setActivePinia } from 'pinia'
import Lightbox from '@/components/lightbox/lightbox.vue'
import LightboxFileActionButtons from '@/components/lightbox/LightboxFileActionButtons.vue'
import DeleteConfirmPopup from '@/components/base/DeleteConfirmPopup.vue'
import VDropdownMenu from '@/components/base/VDropdownMenu.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import VOutlinedButton from '@/components/base/VOutlinedButton.vue'
import type { ISource } from '@/components/lightbox/types'
import { useOpenMedia } from '@/hooks/open-media'
import { DataType } from '@/lib/data'
import { gqlFetch } from '@/lib/api/gql-client'
import { click, clientTemplate, content, flush, nodes, renderView } from '../helpers/vue-node-renderer'

const fixture = vi.hoisted(() => ({ store: undefined as any, main: undefined as any, resolveFileUrlByPath: vi.fn() }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => fixture.store }))
vi.mock('@/stores/main', () => ({ useMainStore: () => fixture.main }))
vi.mock('@/lib/api/file', () => ({ resolveFileUrlByPath: fixture.resolveFileUrlByPath, download: vi.fn(), getFileName: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn(), toastWithAction: vi.fn() }))
vi.mock('@/lib/api/query', () => ({ initLazyQuery: () => ({ fetch: vi.fn() }), fileInfoGQL: '', tagRelationsGQL: '', tagsGQL: '' }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: vi.fn() }))
vi.mock('@/hooks/files', () => ({ useRename: () => ({}) }))
vi.mock('@/hooks/media-play-history', () => ({ useMediaPlayHistory: () => ({}) }))
vi.mock('@/hooks/organize-undo', () => ({ useOrganizeUndo: () => ({ count: ref(0), clear: vi.fn(), record: vi.fn() }) }))
vi.mock('@/hooks/native-resource-url', () => ({ createNativeResourceOwner: () => ({ dispose: vi.fn() }), isNativeResourceUrl: () => false }))
vi.mock('@/lib/api/tauri-window', () => ({ openMediaInWindow: vi.fn(), openImageEditor: vi.fn() }))
vi.mock('@/components/EditValueModal.vue', () => ({ default: {} }))
vi.mock('@/components/DirectoryPickerModal.vue', () => ({ default: {} }))
vi.mock('@/plugins/router', () => ({ replacePath: vi.fn() }))
vi.mock('@/components/lightbox/LightboxMoveToFolders.vue', () => ({ default: { render: () => null } }))
vi.mock('@/components/lightbox/OrganizeConfirmModal.vue', () => ({ default: {} }))
vi.mock('@/components/modal', () => ({ openModal: vi.fn(), promptModal: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

clientTemplate(Lightbox, 'src/components/lightbox/lightbox.vue')
clientTemplate(LightboxFileActionButtons, 'src/components/lightbox/LightboxFileActionButtons.vue')
clientTemplate(DeleteConfirmPopup, 'src/components/base/DeleteConfirmPopup.vue')
clientTemplate(VDropdownMenu, 'src/components/base/VDropdownMenu.vue')
clientTemplate(VFilledButton, 'src/components/base/VFilledButton.vue')
clientTemplate(VOutlinedButton, 'src/components/base/VOutlinedButton.vue')

afterEach(() => vi.unstubAllGlobals())

function mountLightbox(phone: boolean, tablet: boolean, components: Record<string, unknown> = {}) {
  setActivePinia(createPinia())
  fixture.store = defineStore('test-temp', { state: () => ({
    lightbox: { visible: false, sources: [] as ISource[], index: -1, readOnly: false },
    app: { appDir: '/fixture' },
    urlTokenKey: null,
  }) })()
  fixture.main = defineStore('test-main', { state: () => ({ lightboxInfoVisible: true, imageViewQuality: 'fast' }) })()
  vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() })
  const detailSources = vi.fn()
  const Details: FunctionalComponent<{ current?: ISource }> = props => {
    detailSources(props.current?.path)
    return null
  }
  Details.props = ['current']
  const Empty = { render: () => null }
  const Sheet: FunctionalComponent = (_, { slots }) => h('div', [slots.default?.(), slots.footer?.()])
  const Shell = defineComponent({
    setup() {
      provide('isPhone', phone)
      provide('isTablet', tablet)
      return () => h(Lightbox)
    },
  })
  const view = renderView(Shell, {}, {
    LightboxHeader: Empty, LightboxInfo: Details, LightboxFileDetails: Details, LightboxFileTags: Empty,
    BottomSheet: Sheet, VCircularProgress: Empty, LightboxQualityDropdown: Empty, LightboxFileActionButtons: Empty,
    ...components,
  })
  return { ...view, detailSources }
}

it.each([
  { phone: false, tablet: false, label: 'desktop' },
  { phone: true, tablet: false, label: 'phone' },
  { phone: false, tablet: true, label: 'tablet' },
])('waits for the media source before rendering $label details', async ({ phone, tablet }) => {
  const view = mountLightbox(phone, tablet)
  try {
    const source: ISource = { path: '/fixture/track.mp3', name: 'track.mp3', src: '', size: 1, durationMs: 0 }
    let complete!: (url: string) => void
    fixture.resolveFileUrlByPath.mockReturnValueOnce(new Promise<string>(resolve => { complete = resolve }))

    useOpenMedia().open(0, [source])
    await flush()
    expect(fixture.store.lightbox.visible).toBe(true)
    expect(view.detailSources).not.toHaveBeenCalled()

    complete('/fixture/stream.mp3')
    await flush()
    expect(view.detailSources).toHaveBeenCalledWith('/fixture/track.mp3')
  } finally {
    view.unmount()
  }
})

it.each([
  { action: 'delete', operation: 'deleteMediaItems', index: 0, affectedCount: 1, selected: 1 },
  { action: 'delete', operation: 'deleteMediaItems', index: 1, affectedCount: 1, selected: undefined },
  { action: 'delete', operation: 'deleteMediaItems', index: 0, affectedCount: 0, selected: 0 },
  { action: 'move_to_trash', operation: 'trashMediaItems', index: 1, affectedCount: 1, selected: 0 },
])('updates the preview after $operation on item $index with $affectedCount affected items', async ({ action, operation, index, affectedCount, selected }) => {
  vi.stubGlobal('document', { addEventListener: vi.fn(), removeEventListener: vi.fn(), getElementById: vi.fn(), querySelector: vi.fn() })
  vi.stubGlobal('Image', class { removeAttribute() {} })
  const view = mountLightbox(true, false, { LightboxFileActionButtons, DeleteConfirmPopup, VDropdownMenu, VFilledButton, VOutlinedButton })
  fixture.main.imageViewQuality = 'original'
  const sources = [0, 1].map(id => {
    const name = `${id}.png`
    const path = `/fixture/${name}`
    return { name, path, src: path, size: 1, durationMs: 0, type: DataType.IMAGE, data: { id: String(id), name, path }, trashed: action === 'delete' } satisfies ISource
  })
  const button = (label: string) => nodes(view.portal).find(node => node.tag === 'button' && content(node).trim() === label)!
  const image = () => nodes(view.portal).find(node => node.tag === 'img')
  try {
    useOpenMedia().open(index, sources)
    await flush()
    expect(image()?.props.src).toBe(sources[index].path)
    vi.mocked(gqlFetch).mockResolvedValueOnce({ data: { [operation]: { affectedCount } } })
    click(button(action))
    if (action === 'delete') {
      await flush()
      click(button('confirm'))
    }
    await flush()
    expect(gqlFetch).toHaveBeenLastCalledWith(expect.stringContaining(`mutation ${operation}`), { type: DataType.IMAGE, query: `ids:${index}` }, { dedupe: false })
    expect(fixture.store.lightbox.visible).toBe(selected !== undefined)
    expect(image()?.props.src).toBe(selected === undefined ? undefined : `/fixture/${selected}.png`)
    expect(fixture.store.lightbox.sources.map((source: ISource) => source.name)).toEqual(affectedCount ? [`${1 - index}.png`] : ['0.png', '1.png'])
  } finally {
    view.unmount()
  }
})
