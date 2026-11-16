import { expect, it } from 'vitest'
import { useImageEditorDoc } from '@/views/image-editor/composables/useImageEditorDoc'
import { useImageEditorUndo } from '@/views/image-editor/composables/useImageEditorUndo'

it('removes an image layer and its resource together and restores both with one undo', () => {
  const doc = useImageEditorDoc()
  const layer = { id: 'image', type: 'image' as const, visible: true, name: 'Image', x: 20, y: 20, w: 40, h: 40, opacity: 1, rotation: 0 }
  doc.addLayer(layer)
  doc.setImageSrc(layer.id, 'data:image/png;base64,synthetic')
  const history = useImageEditorUndo(doc)
  expect(doc.removeLayer(0)).toEqual(layer)
  expect(doc.getImageSrc(layer.id)).toBeUndefined()
  expect(doc.getLayerCount()).toBe(0)
  expect(history.undoManager.undoStack).toHaveLength(1)
  history.undo()
  expect(doc.getLayers()).toEqual([layer])
  expect(doc.getImageSrc(layer.id)).toBe('data:image/png;base64,synthetic')
  history.dispose(); doc.destroy()
})
