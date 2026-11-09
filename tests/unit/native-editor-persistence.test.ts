import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { useEditorImage, type EditorImageContext } from '@/views/image-editor/composables/useEditorImage'

vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => true }))
vi.mock('@/lib/api/native-client', () => ({ nativeViewScope: vi.fn() }))

let images: Array<{ crossOrigin: string; onload?: () => void; naturalWidth: number; naturalHeight: number }>
const serialize = vi.fn()

function context() {
  const value = {
    sourceImg: ref(null), selectedLayerId: ref(null), activeTool: ref('select'), previewLayer: ref(null), editorActive: ref(false),
    pushUndo: vi.fn(), requestRender: vi.fn(), clearHistory: vi.fn(), persistSourceImage: vi.fn(),
    doc: { ydoc: { transact: vi.fn((operation: () => void) => operation()) }, setCanvasSize: vi.fn(), setImgOffset: vi.fn(), clearLayers: vi.fn() },
  }
  return { value, editor: useEditorImage(value as unknown as EditorImageContext) }
}

beforeEach(() => {
  vi.resetAllMocks()
  images = []
  vi.stubGlobal('Image', class {
    crossOrigin = ''
    naturalWidth = 8
    naturalHeight = 8
    onload?: () => void
    constructor() { images.push(this) }
    set src(_value: string) { queueMicrotask(() => this.onload?.()) }
  })
  vi.stubGlobal('document', { createElement: () => ({ getContext: () => ({ drawImage: vi.fn() }), toDataURL: serialize }) })
})
afterEach(() => vi.unstubAllGlobals())

it.each(['plain-resource://file/owner', 'http://plain-resource.file/owner'])('requires CORS and persists decoded pixels instead of the scoped URL %s', async (url) => {
  serialize.mockReturnValue('data:image/png;base64,generated')
  const { value, editor } = context()
  await editor.loadImageFromUrl(url)
  expect(images[0].crossOrigin).toBe('anonymous')
  expect(value.persistSourceImage).toHaveBeenCalledExactlyOnceWith('data:image/png;base64,generated')
  expect(value.editorActive.value).toBe(true)
})

it('rejects a native canvas failure before changing the editor or persisting the opaque URL', async () => {
  serialize.mockImplementation(() => { throw new Error('Canvas origin is not clean') })
  const { value, editor } = context()
  await expect(editor.loadImageFromUrl('plain-resource://file/owner')).rejects.toThrow('Canvas origin is not clean')
  expect(value.persistSourceImage).not.toHaveBeenCalled()
  expect(value.pushUndo).not.toHaveBeenCalled()
  expect(value.doc.ydoc.transact).not.toHaveBeenCalled()
  expect(value.sourceImg.value).toBeNull()
})

it('preserves the existing external URL behavior on a tainted remote canvas', async () => {
  serialize.mockImplementation(() => { throw new Error('Remote canvas') })
  const { value, editor } = context()
  await editor.loadImageFromUrl('https://images.invalid/remote.png')
  expect(images[0].crossOrigin).toBe('')
  expect(value.persistSourceImage).toHaveBeenCalledExactlyOnceWith('https://images.invalid/remote.png')
})
