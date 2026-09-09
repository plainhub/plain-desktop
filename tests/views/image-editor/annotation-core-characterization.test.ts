import { createApp, h, nextTick } from 'vue'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useImageEditorCore } from '@/views/image-editor/composables/useImageEditorCore'
import { CHARACTERIZATION_LAYERS, createDeterministicSourceImage, renderCharacterizationPixels, sha256Hex } from './annotation-characterization.fixture'

vi.mock('@/views/image-editor/store/plain-app-store', () => ({
  PlainAppProjectStore: class {
    async save() {}
    async load() {
      return null
    }
    async delete() {}
    async list() {
      return []
    }
  },
}))

vi.mock('@/views/image-editor/sync/event-sync-transport', () => ({
  EventSyncTransport: class {
    async connect() {}
    broadcastUpdate() {}
    onUpdate() {
      return () => {}
    }
    destroy() {}
  },
}))

vi.mock('@/views/image-editor/pixi/PixiEditorRenderer', () => ({
  PixiEditorRenderer: class {
    isReady = false
    async init() {
      this.isReady = true
    }
    resize() {}
    setViewport() {}
    sync() {}
    destroy() {
      this.isReady = false
    }
  },
}))

type ImageEditorCore = ReturnType<typeof useImageEditorCore>

const cleanups: Array<() => void> = []

afterEach(() => {
  while (cleanups.length) cleanups.pop()!()
})

async function mountCore(): Promise<ImageEditorCore> {
  let core!: ImageEditorCore
  const replaceState = vi.spyOn(window.history, 'replaceState').mockImplementation(() => {})
  const host = document.createElement('div')
  const app = createApp({
    setup() {
      core = useImageEditorCore()
      return () => h('div')
    },
  })

  app.mount(host)
  await nextTick()
  await Promise.resolve()

  cleanups.push(() => {
    app.unmount()
    replaceState.mockRestore()
  })
  return core
}

function pointer(type: string, x: number, y: number): PointerEvent {
  return new PointerEvent(type, {
    button: 0,
    clientX: x,
    clientY: y,
  })
}

function beginAndFinishDrag(core: ImageEditorCore, tool: 'rect' | 'ellipse' | 'arrow' | 'mosaic', start: { x: number; y: number }, end: { x: number; y: number }) {
  core.tools.activeTool.value = tool
  core.pointer.onPointerDown(pointer('pointerdown', start.x, start.y))
  core.pointer.onPointerMove(pointer('pointermove', end.x, end.y))
  core.pointer.onPointerUp()
}

describe('useImageEditorCore characterization', () => {
  it('retains the public API consumed by ImageEditor.vue', async () => {
    const core = await mountCore()

    expect(Object.keys(core).sort()).toEqual([
      'binding',
      'canvasRef',
      'crop',
      'doc',
      'exportOps',
      'history',
      'image',
      'layerOps',
      'overlayRef',
      'persistence',
      'pointer',
      'render',
      'state',
      'sticker',
      'tools',
      'wrapRef',
    ])
    expect(Object.keys(core.state).sort()).toEqual([
      'bgColor',
      'canvasSize',
      'editorActive',
      'imgAlpha',
      'imgOffset',
      'inlineEditLayerId',
      'isFullscreen',
      'layerImages',
      'layers',
      'renderScale',
      'selectedLayer',
      'selectedLayerId',
      'sourceImg',
    ])
    expect(Object.keys(core.tools).sort()).toEqual(['activeColor', 'activeFontSize', 'activeLineWidth', 'activeTool', 'overlayCursor'])
    expect(Object.keys(core.history).sort()).toEqual(['canRedo', 'canUndo', 'pushUndo', 'redo', 'undo'])
    expect(Object.keys(core.pointer).sort()).toEqual(['onDoubleClick', 'onPointerDown', 'onPointerMove', 'onPointerUp'])
    expect(Object.keys(core.exportOps).sort()).toEqual(['copyToClipboard', 'download', 'getPreviewBlobUrl'])
    expect(Object.keys(core.crop).sort()).toEqual(['applyCrop', 'cancelCrop', 'cropRect', 'isCropping'])
    expect(Object.keys(core.render).sort()).toEqual(['draw', 'resizeCanvas'])
    expect(Object.keys(core.image).sort()).toEqual(['loadImage', 'loadImageFromUrl', 'reset', 'setBgColor', 'setSourceImg', 'startBlank'])
    expect(Object.keys(core.layerOps).sort()).toEqual([
      'addImageLayerFromFile',
      'addStickerLayer',
      'addTextLayer',
      'clearLayers',
      'removeLayer',
      'reorderLayer',
      'replaceImageLayerFile',
      'toggleLayerVisibility',
    ])
    expect(Object.keys(core.sticker).sort()).toEqual(['autoResizeSticker', 'toggleStickerBold', 'toggleStickerItalic', 'updateStickerFontSize', 'updateStickerText'])
    expect(Object.keys(core.persistence).sort()).toEqual(['deleteProject', 'listRecentProjects', 'loadProjectById', 'scheduleSave'])

    expect(core.tools.activeTool.value).toBe('select')
    expect(core.tools.activeColor.value).toBe('#ef4444')
    expect(core.tools.activeLineWidth.value).toBe(4)
    expect(core.tools.activeFontSize.value).toBe(48)
    expect(core.state.canvasSize.value).toEqual({ width: 1920, height: 1080 })
  })

  it('creates every requested annotation layer with current pointer semantics', async () => {
    const core = await mountCore()
    core.doc.setCanvasSize(100, 100)
    core.tools.activeColor.value = '#3b82f6'
    core.tools.activeLineWidth.value = 8

    const canvas = document.createElement('canvas')
    canvas.width = 100
    canvas.height = 100
    canvas.getBoundingClientRect = () => ({
      x: 0,
      y: 0,
      left: 0,
      top: 0,
      right: 100,
      bottom: 100,
      width: 100,
      height: 100,
      toJSON: () => ({}),
    })
    core.canvasRef.value = canvas
    await nextTick()

    beginAndFinishDrag(core, 'rect', { x: 10, y: 12 }, { x: 38, y: 44 })
    beginAndFinishDrag(core, 'ellipse', { x: 42, y: 10 }, { x: 70, y: 34 })
    beginAndFinishDrag(core, 'arrow', { x: 8, y: 80 }, { x: 36, y: 60 })

    core.tools.activeTool.value = 'brush'
    core.pointer.onPointerDown(pointer('pointerdown', 45, 76))
    core.pointer.onPointerMove(pointer('pointermove', 52, 68))
    core.pointer.onPointerMove(pointer('pointermove', 59, 79))
    core.pointer.onPointerUp()

    core.tools.activeTool.value = 'text'
    core.pointer.onPointerDown(pointer('pointerdown', 75, 65))
    beginAndFinishDrag(core, 'mosaic', { x: 72, y: 8 }, { x: 94, y: 30 })

    expect(core.state.layers.map((layer) => layer.type)).toEqual(['rect', 'ellipse', 'arrow', 'freehand', 'text', 'mosaic'])
    expect(core.state.layers[0]).toMatchObject({
      type: 'rect',
      x: 10,
      y: 12,
      w: 28,
      h: 32,
      color: '#3b82f6',
      lineWidth: 8,
    })
    expect(core.state.layers[1]).toMatchObject({
      type: 'ellipse',
      cx: 56,
      cy: 22,
      rx: 14,
      ry: 12,
      color: '#3b82f6',
      lineWidth: 8,
    })
    expect(core.state.layers[2]).toMatchObject({
      type: 'arrow',
      x1: 8,
      y1: 80,
      x2: 36,
      y2: 60,
      color: '#3b82f6',
      lineWidth: 8,
      rotation: 0,
    })
    expect(core.state.layers[3]).toMatchObject({
      type: 'freehand',
      points: [
        { x: 45, y: 76 },
        { x: 52, y: 68 },
        { x: 59, y: 79 },
      ],
      color: '#3b82f6',
      lineWidth: 8,
    })
    expect(core.state.layers[4]).toMatchObject({
      type: 'text',
      x: 75,
      y: 65,
      text: 'Text',
      fontSize: 48,
      color: '#3b82f6',
      maxWidth: 60,
    })
    expect(core.state.layers[5]).toMatchObject({
      type: 'mosaic',
      x: 72,
      y: 8,
      w: 22,
      h: 22,
      blockSize: 12,
    })
    expect(core.tools.activeTool.value).toBe('mosaic')
  })
})

describe('image-editor non-text render characterization', () => {
  it('has an exact-zero same-input noise floor and a stable pixel digest', async () => {
    const source = await createDeterministicSourceImage()
    const first = renderCharacterizationPixels(source)
    const second = renderCharacterizationPixels(source)

    expect(Array.from(second.data)).toEqual(Array.from(first.data))
    expect(await sha256Hex(first.data)).toBe('c1726456f8959f8d905190b73002a119ca99daf3a89cb42ca21625236536effe')
  })

  it('keeps the fixture explicit and limited to requested non-text tools', () => {
    expect(CHARACTERIZATION_LAYERS.map((layer) => layer.type)).toEqual(['mosaic', 'rect', 'ellipse', 'arrow', 'freehand'])
    expect(CHARACTERIZATION_LAYERS.every((layer) => layer.visible)).toBe(true)
  })
})
