import { EditorLayerType } from '@/views/image-editor/utils/types'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { ref, shallowRef, h, type PropType } from 'vue'
import Editor from '@/views/image-editor/components/ImageEditor.vue'
import Background from '@/views/image-editor/components/ImageEditorBackground.vue'
import { useImageEditorDoc } from '@/views/image-editor/composables/useImageEditorDoc'
import { clientTemplate, renderView, nodes, click, flush, content } from '../helpers/vue-node-renderer'

const fixture = vi.hoisted(() => ({ core: {} as any }))
vi.mock('@/views/image-editor/composables/useImageEditorCore', () => ({ useImageEditorCore: () => fixture.core }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => '' }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/HeaderActions.vue', () => ({ default: { render: () => null } }))
clientTemplate(Editor, 'src/views/image-editor/components/ImageEditor.vue')
clientTemplate(Background, 'src/views/image-editor/components/ImageEditorBackground.vue')
let view: ReturnType<typeof renderView>
beforeEach(() => {
  vi.stubGlobal('window', { devicePixelRatio: 1, addEventListener: vi.fn(), removeEventListener: vi.fn() })
  vi.stubGlobal('document', { addEventListener: vi.fn(), removeEventListener: vi.fn() })
  vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} })
  class FixtureImage { src = 'data:image/png;base64,synthetic' }
  vi.stubGlobal('HTMLImageElement', FixtureImage)
  const doc = useImageEditorDoc()
  doc.setSourceImage('data:image/png;base64,synthetic')
  doc.setImgOffset(12, 16)
  fixture.core = {
    canvasRef: ref(null), overlayRef: ref(null), wrapRef: ref(null), doc,
    state: { sourceImg: shallowRef(new FixtureImage()), canvasSize: ref({ width: 320, height: 240 }), bgColor: ref('#ffffff'), imgAlpha: ref(100), imgOffset: { x: 12, y: 16 }, editorActive: ref(true), isFullscreen: ref(false), inlineEditLayerId: ref(null), renderScale: ref(1), layers: [], selectedLayerId: ref(null) },
    tools: { activeTool: ref('select'), activeColor: ref('#ef4444'), activeLineWidth: ref(4), overlayCursor: ref('default') },
    crop: { isCropping: ref(false), cropRect: ref(null), applyCrop: vi.fn(), cancelCrop: vi.fn() },
    history: { undo: vi.fn(), redo: vi.fn(), canUndo: ref(false), canRedo: ref(false), pushUndo: vi.fn() },
    render: { draw: vi.fn(), resizeCanvas: vi.fn() }, image: {}, pointer: {}, layerOps: {}, sticker: {}, exportOps: {}, persistence: {},
  }
  const stub = { render: () => null }
  const Sidebar = { emits: ['selectLayer'], setup(_props: unknown, { emit }: { emit: (event: string, id: string) => void }) { return () => h('button', { onClick: () => emit('selectLayer', 'text') }, 'select-text') } }
  const TextEditor = { props: { layer: Object as PropType<{ id: string } | null> }, emits: ['close'], setup(props: { layer?: { id: string } | null }, { emit }: { emit: (event: string) => void }) { return () => props.layer ? h('button', { onClick: () => emit('close') }, 'close-text') : null } }
  view = renderView(Editor, {}, { ...Object.fromEntries(['ImageEditorToolbar', 'SaveFormatButton', 'VDropdownMenu', 'InlineDeleteConfirm', 'ImageEditorStickerPopup', 'ImageLightbox'].map(name => [name, stub])), ImageEditorBackground: Background, ImageEditorSidebar: Sidebar, TextLayerEditor: TextEditor })
})
afterEach(() => { view?.unmount(); fixture.core.doc.destroy(); vi.unstubAllGlobals() })

it('removes the background from the persisted document through the actual remove button', async () => {
  click(nodes(view.root).find(node => node.props.class === 'remove-btn')!)
  await flush()
  expect(fixture.core.doc.getSourceImage()).toBeNull()
})

it('shows reset for the actual document offset and resets that offset through the button', async () => {
  const reset = nodes(view.root).find(node => node.props.class === 'reset-offset-btn')
  expect(reset).toBeDefined()
  click(reset!)
  await flush()
  expect(fixture.core.doc.getImgOffset()).toEqual({ x: 0, y: 0 })
})

it('reopens text controls when the same selected layer is clicked after closing them', async () => {
  fixture.core.wrapRef.value.getBoundingClientRect = () => ({ top: 0, right: 600 })
  fixture.core.state.layers.push({ id: 'text', type: EditorLayerType.TEXT, x: 20, y: 20 })
  click(nodes(view.root).find(node => content(node) === 'select-text' && node.tag === 'button')!)
  await flush()
  click(nodes(view.root).find(node => content(node) === 'close-text' && node.tag === 'button')!)
  await flush()
  click(nodes(view.root).find(node => content(node) === 'select-text' && node.tag === 'button')!)
  await flush()
  expect(nodes(view.root).some(node => node.tag === 'button' && content(node) === 'close-text')).toBe(true)
})
