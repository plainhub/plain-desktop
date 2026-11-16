import { afterEach, expect, it, vi } from 'vitest'
import { defineComponent } from 'vue'
import { useImageEditorDoc } from '@/views/image-editor/composables/useImageEditorDoc'
import { useImageEditorDocBinding } from '@/views/image-editor/composables/useImageEditorDocBinding'
import type { EditorTextLayer } from '@/views/image-editor/utils/types'
import { useTextLayerEditor } from '@/views/image-editor/composables/useTextLayerEditor'
import { renderView, flush } from '../helpers/vue-node-renderer'

vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
afterEach(() => vi.unstubAllGlobals())

it('changes only the selected text field and keeps an opened settings section open', async () => {
  vi.stubGlobal('window', { innerWidth: 1440, innerHeight: 960, addEventListener() {}, removeEventListener() {} })
  vi.stubGlobal('document', { addEventListener() {}, removeEventListener() {} })
  const doc = useImageEditorDoc()
  doc.addLayer({ id: 'text', type: 'text', visible: true, name: 'Text', x: 10, y: 10, text: 'Text', fontSize: 24, color: '#000000', fontFamily: 'sans-serif', fontWeight: 'normal', fontStyle: 'normal', textAlign: 'left', letterSpacing: 0, textDecoration: 'none', textStroke: { width: 0, color: '#000000' }, textShadow: { offsetX: 0, offsetY: 0, blur: 0, color: '#000000' }, glow: false, rotation: 0, maxWidth: 100 })
  const binding = useImageEditorDocBinding(doc)
  let controls!: ReturnType<typeof useTextLayerEditor>
  const view = renderView(defineComponent({ setup() {
    controls = useTextLayerEditor(() => binding.layers[0] as EditorTextLayer, () => ({ top: 8, left: 8 }), doc, vi.fn())
    return () => null
  } }))
  const changes: string[][] = []
  doc.getLayerYMap('text')!.observe(event => changes.push([...event.keysChanged]))
  controls.showShadow.value = true
  controls.setProperty('fontSize', 48)
  await flush()
  expect(changes).toEqual([['fontSize']])
  expect(controls.showShadow.value).toBe(true)
  expect(doc.getLayer('text')).toMatchObject({ fontSize: 48, text: 'Text', color: '#000000' })
  view.unmount(); binding.dispose(); doc.destroy()
})
