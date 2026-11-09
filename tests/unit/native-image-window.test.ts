import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import * as Vue from 'vue'
import { createRenderer, defineComponent, nextTick, ssrContextKey } from 'vue'
import { compileScript, compileTemplate, parse } from 'vue/compiler-sfc'

const fixture = vi.hoisted(() => ({
  image: vi.fn(), createUrl: vi.fn(), revokeUrl: vi.fn(), query: {} as Record<string, string>, open: vi.fn(), close: vi.fn(), load: vi.fn(), loadFile: vi.fn(), blank: vi.fn(), title: vi.fn(), toast: vi.fn(),
  store: { lightbox: { sources: [] as unknown[], index: -1, visible: false, readOnly: false } },
}))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/toaster', () => ({ default: fixture.toast }))
vi.mock('vue-router', () => ({ useRoute: () => ({ get query() { return fixture.query } }) }))
vi.mock('@/hooks/native-resource-url', () => ({
  nativeResourceUrl: fixture.open,
  nativeResourceImage: fixture.image,
  closeNativeResource: fixture.close,
  isNativeResourceUrl: (url: string) => /^(plain-resource:\/\/file\/|https?:\/\/plain-resource\.file\/)/.test(url),
}))
vi.mock('@/lib/api/tauri-window', () => ({ setWindowDeviceName: fixture.title }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => fixture.store }))
vi.mock('@/views/image-editor/components/ImageEditor.vue', async () => ({
  default: (await import('vue')).defineComponent({
    setup(_, { expose }) {
      expose({ loadImageFromUrl: fixture.load, loadImage: fixture.loadFile, startBlank: fixture.blank })
      return () => null
    },
  }),
}))

import ImageEditorView from '@/views/image-editor/ImageEditorView.vue'
import editorSource from '@/views/image-editor/ImageEditorView.vue?raw'
import MediaPreviewView from '@/views/media-preview/MediaPreviewView.vue'

const { descriptor } = parse(editorSource)
const template = compileTemplate({
  source: descriptor.template!.content, filename: 'ImageEditorView.vue', id: 'native-editor',
  compilerOptions: { mode: 'function', prefixIdentifiers: true, bindingMetadata: compileScript(descriptor, { id: 'native-editor' }).bindings },
})
ImageEditorView.render = new Function('Vue', template.code)(Vue)
MediaPreviewView.render = () => null

const renderer = createRenderer<object, object>({
  insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
})
let unmount: (() => void) | undefined
function mount(component: typeof ImageEditorView | typeof MediaPreviewView) {
  const app = renderer.createApp(component)
  app.provide(ssrContextKey, { modules: new Set() })
  app.component('VCircularProgress', defineComponent({ render: () => null }))
  app.mount({})
  unmount = () => app.unmount()
}
async function flush() {
  for (let i = 0; i < 8; i++) await nextTick()
}

beforeEach(() => {
  vi.resetAllMocks()
  fixture.query = {}
  fixture.store.lightbox = { sources: [], index: -1, visible: false, readOnly: false }
  fixture.open.mockResolvedValue('plain-resource://file/new-owner')
  fixture.image.mockResolvedValue(new Blob(['image'], { type: 'image/png' }))
  fixture.createUrl.mockReturnValue('blob:editor-image')
  vi.spyOn(URL, 'createObjectURL').mockImplementation(fixture.createUrl)
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(fixture.revokeUrl)
  fixture.close.mockResolvedValue(undefined)
  fixture.load.mockResolvedValue(undefined)
  vi.stubGlobal('__IS_TAURI__', true)
  vi.stubGlobal('window', { location: { search: '', origin: 'tauri://localhost' } })
  vi.stubGlobal('fetch', vi.fn())
  vi.spyOn(console, 'warn').mockImplementation(() => {})
})
afterEach(() => {
  unmount?.(); unmount = undefined
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

it('loads bounded IPC image bytes through a Blob URL and revokes the URL after decoding', async () => {
  fixture.query = { reference: 'fid:original-image', name: 'image.png' }
  mount(ImageEditorView)
  await flush()
  expect(fixture.image).toHaveBeenCalledExactlyOnceWith('fid:original-image', expect.any(AbortSignal))
  expect(fixture.load).toHaveBeenCalledExactlyOnceWith('blob:editor-image')
  expect(fixture.revokeUrl).toHaveBeenCalledExactlyOnceWith('blob:editor-image')
  expect(fetch).not.toHaveBeenCalled()
  expect(fixture.loadFile).not.toHaveBeenCalled()
})

it.each(['unavailable', 'decode failed'])('does not fall back to HTTP when IPC image loading is %s', async (reason) => {
  fixture.query = { reference: 'app://image.png' }
  if (reason === 'unavailable') fixture.image.mockRejectedValue(new Error(reason))
  else fixture.load.mockRejectedValue(new Error(reason))
  mount(ImageEditorView)
  await flush()
  expect(fixture.revokeUrl).toHaveBeenCalledTimes(reason === 'unavailable' ? 0 : 1)
  expect(fetch).not.toHaveBeenCalled()
  expect(fixture.loadFile).not.toHaveBeenCalled()
  expect(console.warn).toHaveBeenCalledWith('[ImageEditorView] Failed to load native image', expect.any(Error))
  expect(fixture.toast).toHaveBeenCalledExactlyOnceWith('failed_to_load_file', 'error')
})

it('rejects a foreign editor handle without trying it or converting it to HTTP', async () => {
  fixture.query = { src: 'http://plain-resource.file/other-owner' }
  mount(ImageEditorView)
  await flush()
  expect(fixture.open).not.toHaveBeenCalled()
  expect(fixture.load).not.toHaveBeenCalled()
  expect(fetch).not.toHaveBeenCalled()
  expect(console.warn).toHaveBeenCalled()
  expect(fixture.toast).toHaveBeenCalledExactlyOnceWith('failed_to_load_file', 'error')
})

it('does not open or load an editor reference after the view was unmounted', async () => {
  fixture.query = { reference: 'fid:original-image' }
  mount(ImageEditorView)
  unmount?.(); unmount = undefined
  await flush()
  expect(fixture.open).not.toHaveBeenCalled()
  expect(fixture.load).not.toHaveBeenCalled()
})

it('reopens a path-only native preview in the destination owner', async () => {
  window.location.search = '?path=fid%3Aimage&name=image.png&size=4'
  mount(MediaPreviewView)
  await flush()
  expect(fixture.open).toHaveBeenCalledExactlyOnceWith('fid:image')
  expect(fixture.store.lightbox.sources).toEqual([expect.objectContaining({ src: 'plain-resource://file/new-owner', path: 'fid:image', size: 4 })])
  expect(fixture.title).toHaveBeenCalledExactlyOnceWith('image.png')
  unmount?.(); unmount = undefined
  expect(fixture.close).toHaveBeenCalledExactlyOnceWith('fid:image')
})

it('does not reopen external preview paths as local files', async () => {
  window.location.search = '?src=https%3A%2F%2Fdevice.invalid%2Ffs%3Fid%3Dimage&path=%2Fremote%2Fimage.png'
  mount(MediaPreviewView)
  await flush()
  expect(fixture.open).not.toHaveBeenCalled()
  expect(fixture.store.lightbox.sources).toEqual([expect.objectContaining({ src: 'https://device.invalid/fs?id=image', path: '/remote/image.png' })])
})

it('does not install a pending native preview after destination unmount', async () => {
  window.location.search = '?path=fid%3Aimage'
  let finish!: (url: string) => void
  fixture.open.mockImplementation(() => new Promise<string>((resolve) => { finish = resolve }))
  mount(MediaPreviewView)
  unmount?.(); unmount = undefined
  finish('plain-resource://file/new-owner')
  await flush()
  expect(fixture.store.lightbox.sources).toEqual([])
  expect(fixture.close).toHaveBeenCalledExactlyOnceWith('fid:image')
  expect(fixture.title).not.toHaveBeenCalled()
})


it('reports an invalid native reference without opening or closing a foreign handle', async () => {
  fixture.query = { reference: 'plain-resource://file/foreign-owner' }
  mount(ImageEditorView)
  await flush()
  expect(fixture.open).not.toHaveBeenCalled()
  expect(fixture.close).not.toHaveBeenCalled()
  expect(fixture.toast).toHaveBeenCalledExactlyOnceWith('failed_to_load_file', 'error')
})

it('does not report a failed editor load after cancellation by unmount', async () => {
  fixture.query = { reference: 'fid:original-image' }
  let fail!: (error: Error) => void
  fixture.image.mockImplementation(() => new Promise<Blob>((_, reject) => { fail = reject }))
  mount(ImageEditorView)
  await flush()
  unmount?.(); unmount = undefined
  fail(new Error('resource_closed'))
  await flush()
  expect(fixture.toast).not.toHaveBeenCalled()
  expect(fixture.load).not.toHaveBeenCalled()
  expect(fixture.image.mock.calls[0][1].aborted).toBe(true)
})
