import { afterEach, expect, it, vi } from 'vitest'
import { defineComponent, h, provide, ref, type FunctionalComponent } from 'vue'
import { createPinia, defineStore, setActivePinia } from 'pinia'
import Lightbox from '@/components/lightbox/lightbox.vue'
import type { ISource } from '@/components/lightbox/types'
import { useOpenMedia } from '@/hooks/open-media'
import { clientTemplate, flush, renderView } from '../helpers/vue-node-renderer'

const fixture = vi.hoisted(() => ({ store: undefined as any, main: undefined as any, resolveFileUrlByPath: vi.fn() }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => fixture.store }))
vi.mock('@/stores/main', () => ({ useMainStore: () => fixture.main }))
vi.mock('@/lib/api/file', () => ({ resolveFileUrlByPath: fixture.resolveFileUrlByPath, download: vi.fn(), getFileName: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn(), toastWithAction: vi.fn() }))
vi.mock('@/lib/api/query', () => ({ initLazyQuery: () => ({ fetch: vi.fn() }), fileInfoGQL: '', tagRelationsGQL: '', tagsGQL: '' }))
vi.mock('@/lib/api/mutation', () => ({ initMutation: () => ({ mutate: vi.fn(), onError: vi.fn() }), deleteFilesGQL: '' }))
vi.mock('@/hooks/files', () => ({ useRename: () => ({}) }))
vi.mock('@/hooks/media', () => ({ useDeleteMediaItem: vi.fn() }))
vi.mock('@/hooks/media-play-history', () => ({ useMediaPlayHistory: () => ({}) }))
vi.mock('@/hooks/organize-undo', () => ({ useOrganizeUndo: () => ({ count: ref(0) }) }))
vi.mock('@/hooks/native-resource-url', () => ({ createNativeResourceOwner: () => ({ dispose: vi.fn() }), isNativeResourceUrl: () => false }))
vi.mock('@/lib/api/tauri-window', () => ({ openMediaInWindow: vi.fn(), openImageEditor: vi.fn() }))
vi.mock('@/components/EditValueModal.vue', () => ({ default: {} }))
vi.mock('@/components/lightbox/LightboxMoveToFolders.vue', () => ({ default: {} }))
vi.mock('@/components/lightbox/OrganizeConfirmModal.vue', () => ({ default: {} }))
vi.mock('@/components/modal', () => ({ openModal: vi.fn(), promptModal: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

clientTemplate(Lightbox, 'src/components/lightbox/lightbox.vue')

afterEach(() => vi.unstubAllGlobals())

it.each([
  { phone: false, tablet: false, label: 'desktop' },
  { phone: true, tablet: false, label: 'phone' },
  { phone: false, tablet: true, label: 'tablet' },
])('waits for the media source before rendering $label details', async ({ phone, tablet }) => {
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
  })

  try {
    const source: ISource = { path: '/fixture/track.mp3', name: 'track.mp3', src: '', size: 1, durationMs: 0 }
    let complete!: (url: string) => void
    fixture.resolveFileUrlByPath.mockReturnValueOnce(new Promise<string>(resolve => { complete = resolve }))

    useOpenMedia().open(0, [source])
    await flush()
    expect(fixture.store.lightbox.visible).toBe(true)
    expect(detailSources).not.toHaveBeenCalled()

    complete('/fixture/stream.mp3')
    await flush()
    expect(detailSources).toHaveBeenCalledWith('/fixture/track.mp3')
  } finally {
    view.unmount()
  }
})
