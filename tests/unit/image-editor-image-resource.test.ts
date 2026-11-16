import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { useImageEditorDoc } from '@/views/image-editor/composables/useImageEditorDoc'
import { useImageEditorLayers } from '@/views/image-editor/composables/useImageEditorLayers'

const encode = vi.fn()
beforeEach(() => {
  encode.mockReset().mockReturnValue('data:image/png;base64,portable')
  vi.stubGlobal('FileReader', class {
    result = 'data:image/svg+xml;base64,synthetic'
    onload?: () => void
    readAsDataURL() { queueMicrotask(() => this.onload?.()) }
  })
  vi.stubGlobal('Image', class {
    naturalWidth = 320; naturalHeight = 240
    onload?: () => void
    set src(_value: string) { queueMicrotask(() => this.onload?.()) }
  })
  vi.stubGlobal('document', { createElement: () => ({ getContext: () => ({ drawImage() {} }), toDataURL: encode }) })
})
afterEach(() => vi.unstubAllGlobals())
function editor() {
  const doc = useImageEditorDoc()
  const layers = useImageEditorLayers([], new Map(), ref({ width: 320, height: 240 }), ref(null), ref('#ef4444'), ref(4), ref(24), doc, vi.fn())
  return { doc, layers }
}
it('stores decoded image layers as portable PNG resources', async () => {
  const { doc, layers } = editor()
  await layers.addImageLayerFromFile({} as File)
  expect(doc.getImageEntries().map(([, src]) => src)).toEqual(['data:image/png;base64,portable'])
  doc.destroy()
})
it('does not insert a layer when its pixels cannot be serialized', async () => {
  encode.mockImplementation(() => { throw new Error('Cannot serialize') })
  const { doc, layers } = editor()
  await expect(layers.addImageLayerFromFile({} as File)).rejects.toThrow('Cannot serialize')
  expect(doc.getLayerCount()).toBe(0)
  expect(doc.getImageEntries()).toEqual([])
  doc.destroy()
})
