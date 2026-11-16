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

it('rejects an unknown layer kind before applying a saved document', () => {
  const source = new Y.Doc()
  source.getMap('meta').set('formatVersion', 1)
  const layer = new Y.Map<unknown>()
  layer.set('id', 'unknown'); layer.set('type', 'unsupported')
  source.getArray('layers').push([layer])
  const doc = useImageEditorDoc()
  expect(() => doc.loadState(Y.encodeStateAsUpdate(source))).toThrow('Unsupported image editor layer type')
  expect(doc.getLayerCount()).toBe(0)
  doc.destroy(); source.destroy()
})
