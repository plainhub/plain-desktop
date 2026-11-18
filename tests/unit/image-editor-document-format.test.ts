import { expect, it } from 'vitest'
import * as Y from 'yjs'
import { useImageEditorDoc } from '@/views/image-editor/composables/useImageEditorDoc'

it('writes the explicit current document format version', () => {
  const doc = useImageEditorDoc()
  const decoded = new Y.Doc()
  Y.applyUpdate(decoded, doc.getStateUpdate())
  expect(decoded.getMap('meta').get('formatVersion')).toBe(1)
  doc.destroy(); decoded.destroy()
})

it.each([undefined, 0, 2])('rejects format %s without changing the active document', version => {
  const source = new Y.Doc()
  const meta = source.getMap('meta')
  if (version !== undefined) meta.set('formatVersion', version)
  meta.set('canvasWidth', 999)
  const doc = useImageEditorDoc()
  doc.setCanvasSize(320, 240)
  expect(() => doc.loadState(Y.encodeStateAsUpdate(source))).toThrow('Unsupported image editor document format')
  expect(doc.getCanvasSize()).toEqual({ width: 320, height: 240 })
  doc.destroy(); source.destroy()
})

it.each(['sticker', 'UNSUPPORTED'])('rejects noncanonical layer kind %s before applying a saved document', type => {
  const source = new Y.Doc()
  source.getMap('meta').set('formatVersion', 1)
  const layer = new Y.Map<unknown>()
  layer.set('id', 'unknown'); layer.set('type', type)
  source.getArray('layers').push([layer])
  const doc = useImageEditorDoc()
  expect(() => doc.loadState(Y.encodeStateAsUpdate(source))).toThrow('Unsupported image editor layer type')
  expect(doc.getLayerCount()).toBe(0)
  doc.destroy(); source.destroy()
})

it('round-trips canonical Yjs enum values from layer creation and edits', () => {
  const doc = useImageEditorDoc()
  doc.addLayer({
    id: 'sticker', type: 'sticker', name: 'Sticker', visible: true,
    x: 12, y: 18, w: 160, h: 90, color: '#ffff00', text: 'note',
    fontSize: 20, fontWeight: '600', fontStyle: 'normal', rotation: 0,
  })
  doc.addLayer({
    id: 'text', type: 'text', name: 'Text', visible: true,
    x: 24, y: 36, text: 'title', fontFamily: 'sans-serif', fontSize: 20, color: '#000000',
    fontWeight: 'normal', fontStyle: 'normal', textAlign: 'center', textDecoration: 'none',
    letterSpacing: 0, glow: false, rotation: 0, maxWidth: 150,
    textStroke: { width: 0, color: '#000000' },
    textShadow: { offsetX: 0, offsetY: 0, blur: 0, color: '#000000' },
  })
  doc.setLayerProp('text', 'textAlign', 'right')
  doc.setLayerProps('text', { fontStyle: 'italic', textDecoration: 'underline', fontWeight: 'bold' })
  doc.moveLayerBy('sticker', 2, 3)

  const bytes = doc.getStateUpdate()
  const raw = new Y.Doc()
  Y.applyUpdate(raw, bytes)
  const [sticker, title] = raw.getArray<Y.Map<unknown>>('layers').toJSON()
  expect(sticker).toMatchObject({ type: 'STICKER', fontWeight: 'SEMIBOLD', fontStyle: 'NORMAL', x: 14, y: 21 })
  expect(title).toMatchObject({ type: 'TEXT', fontWeight: 'BOLD', fontStyle: 'ITALIC', textAlign: 'RIGHT', textDecoration: 'UNDERLINE', fontFamily: 'sans-serif' })

  const restored = useImageEditorDoc()
  restored.loadState(bytes)
  expect(restored.getLayers()).toEqual(doc.getLayers())
  expect(restored.getLayer('sticker')).toMatchObject({ type: 'sticker', fontWeight: '600', fontStyle: 'normal' })

  raw.getArray<Y.Map<unknown>>('layers').get(0).set('fontStyle', 'normal')
  expect(() => restored.loadState(Y.encodeStateAsUpdate(raw))).toThrow('Unsupported image editor fontStyle')
  expect(restored.getLayers()).toEqual(doc.getLayers())
  doc.destroy(); raw.destroy(); restored.destroy()
})
