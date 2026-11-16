import { expect, it } from 'vitest'
import { useImageEditorDoc } from '@/views/image-editor/composables/useImageEditorDoc'
import { useImageEditorDocBinding } from '@/views/image-editor/composables/useImageEditorDocBinding'
import * as Y from 'yjs'

it('persists background controls through the document binding and restores the same values', () => {
  const doc = useImageEditorDoc()
  const binding = useImageEditorDocBinding(doc)
  binding.bgColor.value = '#22c55e'
  binding.imgAlpha.value = 40
  expect(doc.getBgColor()).toBe('#22c55e')
  expect(doc.getImgAlpha()).toBe(40)
  const restored = useImageEditorDoc()
  restored.loadState(doc.getStateUpdate())
  expect(restored.getBgColor()).toBe('#22c55e')
  expect(restored.getImgAlpha()).toBe(40)
  doc.setBgColor('#ef4444')
  expect(binding.bgColor.value).toBe('#ef4444')
  binding.dispose(); doc.ydoc.destroy(); restored.ydoc.destroy()
})

it('reflects nested text-style changes from the document without replacing the layer', () => {
  const doc = useImageEditorDoc()
  doc.addLayer({ id: 'text', type: 'text', visible: true, name: 'Text', x: 10, y: 10, text: 'Text', fontSize: 24, color: '#000000', fontFamily: 'sans-serif', fontWeight: 'normal', fontStyle: 'normal', textAlign: 'left', letterSpacing: 0, textDecoration: 'none', textStroke: { width: 0, color: '#000000' }, textShadow: { offsetX: 0, offsetY: 0, blur: 0, color: '#000000' }, glow: false, rotation: 0, maxWidth: 100 })
  const binding = useImageEditorDocBinding(doc)
  const stroke = doc.getLayerYMap('text')!.get('textStroke') as Y.Map<unknown>
  stroke.set('width', 4)
  expect(binding.layers[0]).toMatchObject({ textStroke: { width: 4, color: '#000000' } })
  binding.dispose(); doc.destroy()
})
