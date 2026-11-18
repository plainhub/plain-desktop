import { EditorLayerType, EditorFontWeight, EditorFontStyle, EditorTextAlign, EditorTextDecoration } from '@/views/image-editor/utils/types'
import { afterEach, expect, it, vi } from 'vitest'
import { defineComponent, h, reactive } from 'vue'
import Sidebar from '@/views/image-editor/components/ImageEditorSidebar.vue'
import { useImageEditorDoc } from '@/views/image-editor/composables/useImageEditorDoc'
import { clientTemplate, renderView, nodes, fire, flush } from '../helpers/vue-node-renderer'

const drag = vi.hoisted(() => ({ start: vi.fn(), end: undefined as undefined | ((from: number, to: number) => void) }))
vi.mock('@/hooks/use-drag-sort', () => ({ useDragSort: (options: { onEnd: (from: number, to: number) => void }) => {
  drag.end = options.onEnd
  return { fromIndex: -1, toIndex: -1, itemStyle: () => ({}), start: drag.start }
} }))
clientTemplate(Sidebar, 'src/views/image-editor/components/ImageEditorSidebar.vue')
let view: ReturnType<typeof renderView>
afterEach(() => view?.unmount())

it('writes sidebar text edits into the document instead of only mutating a display copy', async () => {
  const doc = useImageEditorDoc()
  doc.addLayer({ id: 'text', type: EditorLayerType.TEXT, name: 'Text', visible: true, x: 10, y: 10, text: 'Before', fontSize: 24, color: '#000000', fontFamily: 'sans-serif', fontWeight: EditorFontWeight.NORMAL, fontStyle: EditorFontStyle.NORMAL, textAlign: EditorTextAlign.LEFT, letterSpacing: 0, textDecoration: EditorTextDecoration.NONE, textStroke: { width: 0, color: '#000000' }, textShadow: { offsetX: 0, offsetY: 0, blur: 0, color: '#000000' }, glow: false, rotation: 0, maxWidth: 100 })
  const layers = reactive(doc.getLayers())
  view = renderView(defineComponent({ setup: () => () => h(Sidebar, { layers, canvasWidth: 320, canvasHeight: 240, selectedLayerId: 'text', onUpdateLayerText: (id: string, text: string) => doc.setLayerProp(id, 'text', text) }) }))
  const input = nodes(view.root).find(node => node.props.class === 'layer-text-input')!
  Object.assign(input, { value: 'After' })
  fire(input, 'onUpdate:modelValue', 'After')
  fire(input, 'onInput', { target: { value: 'After' } })
  await flush()
  expect(doc.getLayer('text')).toMatchObject({ text: 'After' })
  doc.destroy()
})

it('uses visible indices while dragging and converts the resulting order to document indices', () => {
  drag.start.mockClear()
  const layers = ['first', 'second'].map(id => ({ id, type: EditorLayerType.RECT, visible: true, name: id, x: 0, y: 0, w: 40, h: 40, color: '#ef4444', lineWidth: 4 }))
  const reordered = vi.fn()
  view = renderView(Sidebar, { layers, canvasWidth: 320, canvasHeight: 240, selectedLayerId: null, onReorderLayer: reordered })
  const first = nodes(view.root).find(node => node.props.class?.includes('layer-item-wrap'))!
  fire(first, 'onPointerdown', {})
  expect(drag.start.mock.calls[0]![1]).toBe(0)
  drag.end!(0, 1)
  expect(reordered).toHaveBeenCalledExactlyOnceWith(1, 0)
})
