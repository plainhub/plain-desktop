import { expect, it } from 'vitest'
import { useImageEditorDoc } from '@/views/image-editor/composables/useImageEditorDoc'

it('reorders integrated layers without losing their properties or nested future fields', () => {
  const doc = useImageEditorDoc()
  for (const id of ['first', 'second']) doc.addLayer({ id, type: 'rect', visible: true, name: id, x: 10, y: 20, w: 30, h: 40, color: '#ef4444', lineWidth: 4 })
  doc.setLayerProp('first', 'extension', { value: 42 })
  doc.moveLayer(0, 1)
  expect(doc.getLayers().map(layer => layer.id)).toEqual(['second', 'first'])
  const restored = useImageEditorDoc()
  restored.loadState(doc.getStateUpdate())
  expect(restored.getLayers().map(layer => layer.id)).toEqual(['second', 'first'])
  expect(restored.getLayerYMap('first')!.toJSON().extension).toEqual({ value: 42 })
  doc.ydoc.destroy(); restored.ydoc.destroy()
})
