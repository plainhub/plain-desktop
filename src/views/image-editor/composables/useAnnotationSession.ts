import { computed, ref, watch, type WatchHandle } from 'vue'
import type { EditorLayer, EditorRasterSource, EditorTextLayer, EditorTool, FreehandLayer } from '@/views/image-editor/utils/types'
import { getEditorLayerBounds, getEditorRasterSize, hitTestLayer, isEditorTextLayer, LINE_WIDTHS, TOOL_COLORS } from '@/views/image-editor/utils/types'
import { drawHoverRect, drawSelectionRect } from '@/views/image-editor/utils/renderer'
import { updateLayerFromDrag } from '@/views/image-editor/utils/drag-helpers'
import { drawCenterCrosshair, drawDistanceGuides } from '@/views/image-editor/utils/overlay-guides'
import { shortUUID } from '@/lib/strutil'
import { PixiEditorRenderer } from '../pixi/PixiEditorRenderer'
import { useEditorCrop } from './useEditorCrop'
import { useEditorImage } from './useEditorImage'
import { useEditorTransform } from './useEditorTransform'
import { useImageEditorDoc } from './useImageEditorDoc'
import { useImageEditorDocBinding } from './useImageEditorDocBinding'
import { useImageEditorExport, type EditorExportRect, type EditorExportResult } from './useImageEditorExport'
import { useImageEditorLayers, nextLayerName, resetLayerCounter } from './useImageEditorLayers'
import { useImageEditorSticker } from './useImageEditorSticker'
import { useImageEditorUndo } from './useImageEditorUndo'
import { RenderScheduler } from './useRenderScheduler'

export interface AnnotationSource {
  drawable: EditorRasterSource
  width: number
  height: number
  release?: () => void
}

export interface AnnotationSessionOptions {
  /** Full-editor compatibility hook. Omit it for an ephemeral session. */
  persistSourceImage?: (value: string | null) => void
  /** Full-editor compatibility hook. Omit it for an ephemeral session. */
  onReset?: () => void
  allowSourceMove?: boolean
  allowDestructiveCrop?: boolean
  /** Capture sessions draft text; the full editor opts into legacy immediate insertion. */
  textCommitMode?: 'draft' | 'immediate'
}

export type AnnotationSessionError = 'disposed' | 'preview-not-ready'
export type AnnotationSessionResult<T> = { ok: true; value: T } | { ok: false; error: AnnotationSessionError }

export function useAnnotationSession(options: AnnotationSessionOptions = {}) {
  const canvasRef = ref<HTMLCanvasElement | null>(null)
  const overlayRef = ref<HTMLCanvasElement | null>(null)
  const wrapRef = ref<HTMLElement | null>(null)

  const doc = useImageEditorDoc()
  const binding = useImageEditorDocBinding(doc)
  const undo = useImageEditorUndo(doc)
  const pixi = new PixiEditorRenderer()
  const scheduler = new RenderScheduler()

  const { canvasSize, bgColor, imgOffset, imgAlpha, sourceImg, layers, layerImages } = binding
  const { canUndo, canRedo, pushUndo, beginGesture, endGesture, cancelGesture, undo: undoFn, redo: redoFn, clearHistory } = undo

  const editorActive = ref(false)
  const renderScale = ref(1)
  const activeTool = ref<EditorTool>('select')
  const activeColor = ref(TOOL_COLORS[0]!)
  const activeLineWidth = ref(LINE_WIDTHS[1]!)
  const activeFontSize = ref(48)
  const selectedLayerId = ref<string | null>(null)
  const isDrawing = ref(false)
  const drawStart = ref<{ x: number; y: number } | null>(null)
  const previewLayer = ref<EditorLayer | null>(null)
  const textDraft = ref<EditorTextLayer | null>(null)
  const isDraggingImage = ref(false)
  const imgDragStart = ref<{ mx: number; my: number; ox: number; oy: number } | null>(null)
  const isDraggingLayer = ref(false)
  const layerDragStart = ref<{ mx: number; my: number } | null>(null)
  const isFullscreen = ref(true)
  const inlineEditLayerId = ref<string | null>(null)
  const hoveredLayerId = ref<string | null>(null)

  const selectedLayer = computed(() => layers.find((layer) => layer.id === selectedLayerId.value) ?? null)

  let disposed = false
  let previewGeneration = 0
  let releaseSource: (() => void) | undefined
  const watchHandles: WatchHandle[] = []

  function releaseCurrentSource() {
    const release = releaseSource
    releaseSource = undefined
    try {
      release?.()
    } catch {
      // Source ownership still ends here even if a platform release hook fails.
    }
  }

  function requestRender() {
    if (!disposed) scheduler.requestMain()
  }

  function requestOverlay() {
    if (!disposed) scheduler.requestOverlay()
  }

  const transform = useEditorTransform(layers, selectedLayerId, doc, beginGesture)
  const { overlayCursor } = transform
  const crop = useEditorCrop(
    canvasSize,
    sourceImg,
    imgOffset,
    layers,
    bgColor,
    layerImages,
    imgAlpha,
    doc,
    pushUndo,
    activeTool,
    requestRender,
    options.persistSourceImage,
    options.allowDestructiveCrop ?? false
  )
  const { isCropping, cropRect } = crop

  watchHandles.push(
    watch(activeTool, (tool) => {
      if (tool !== 'crop') return
      if (!options.allowDestructiveCrop) {
        activeTool.value = 'select'
        return
      }
      cropRect.value = null
      isCropping.value = true
      requestRender()
    })
  )

  function draw() {
    if (disposed || !pixi.isReady) return
    pixi.sync({
      canvasSize: canvasSize.value,
      bgColor: bgColor.value,
      sourceImg: sourceImg.value,
      imgOffset: { x: imgOffset.x, y: imgOffset.y },
      imgAlpha: imgAlpha.value,
      layers,
      layerImages,
      previewLayer: previewLayer.value,
      hideLayerId: inlineEditLayerId.value,
    })
  }

  function drawOverlay() {
    if (disposed) return
    const overlay = overlayRef.value
    if (!overlay) return
    const ctx = overlay.getContext('2d')
    if (!ctx) return
    const scale = renderScale.value
    ctx.setTransform(scale, 0, 0, scale, 0, 0)
    ctx.clearRect(0, 0, canvasSize.value.width, canvasSize.value.height)
    crop.drawCropIfActive(ctx)
    if (inlineEditLayerId.value) return

    const hovered = hoveredLayerId.value
    if (hovered && hovered !== selectedLayerId.value) {
      const layer = layers.find((candidate) => candidate.id === hovered)
      if (layer?.visible) drawHoverRect(ctx, layer)
    }

    const selected = selectedLayer.value
    if (selected?.visible) {
      drawSelectionRect(ctx, selected, scale)
      if (isDraggingLayer.value) {
        drawCenterCrosshair(ctx, canvasSize.value.width, canvasSize.value.height)
        drawDistanceGuides(ctx, getEditorLayerBounds(selected), canvasSize.value.width, canvasSize.value.height)
      }
    }
  }

  scheduler.setRenderers(draw, drawOverlay)
  watchHandles.push(watch(binding.syncVersion, requestRender))
  watchHandles.push(watch(selectedLayerId, requestOverlay))
  watchHandles.push(watch(hoveredLayerId, requestOverlay))
  watchHandles.push(watch(isDraggingLayer, requestOverlay))

  watchHandles.push(
    watch(canvasRef, async (canvas) => {
      if (!canvas || disposed) return
      const generation = ++previewGeneration
      if (!pixi.isReady) await pixi.init(canvas)
      if (disposed || generation !== previewGeneration) {
        if (disposed) pixi.destroy()
        return
      }
      const width = Math.max(1, Math.round(canvasSize.value.width * renderScale.value))
      const height = Math.max(1, Math.round(canvasSize.value.height * renderScale.value))
      pixi.resize(width, height)
      pixi.setViewport(renderScale.value, 0, 0)
      requestRender()
    })
  )

  watchHandles.push(
    watch(renderScale, () => {
      if (disposed || !pixi.isReady) return
      const width = Math.max(1, Math.round(canvasSize.value.width * renderScale.value))
      const height = Math.max(1, Math.round(canvasSize.value.height * renderScale.value))
      pixi.resize(width, height)
      pixi.setViewport(renderScale.value, 0, 0)
      requestRender()
    })
  )

  const imageApi = useEditorImage({
    doc,
    sourceImg,
    selectedLayerId,
    activeTool,
    previewLayer,
    editorActive,
    pushUndo,
    requestRender,
    clearHistory,
    persistSourceImage: options.persistSourceImage,
    onReset: options.onReset,
  })

  function setSource(source: AnnotationSource) {
    if (disposed) throw new Error('AnnotationSession is disposed')
    if (!Number.isInteger(source.width) || !Number.isInteger(source.height) || source.width <= 0 || source.height <= 0) {
      throw new RangeError('Annotation source dimensions must be positive integers')
    }
    const rasterSize = getEditorRasterSize(source.drawable)
    if (rasterSize.width !== source.width || rasterSize.height !== source.height) {
      throw new RangeError('Annotation source dimensions must match its decoded raster')
    }
    releaseCurrentSource()
    releaseSource = source.release
    sourceImg.value = source.drawable
    doc.ydoc.transact(() => {
      doc.setCanvasSize(source.width, source.height)
      doc.setImgOffset(0, 0)
      doc.setBgColor('transparent')
      doc.clearLayers()
    })
    selectedLayerId.value = null
    previewLayer.value = null
    textDraft.value = null
    resetLayerCounter()
    editorActive.value = true
    clearHistory()
    requestRender()
  }

  function clearSource() {
    releaseCurrentSource()
    sourceImg.value = null
    textDraft.value = null
    previewLayer.value = null
    requestRender()
  }

  function clientToCanvas(event: PointerEvent): { x: number; y: number } | null {
    const canvas = canvasRef.value
    if (!canvas) return null
    const rect = canvas.getBoundingClientRect()
    if (rect.width <= 0 || rect.height <= 0) return null
    return {
      x: ((event.clientX - rect.left) * canvasSize.value.width) / rect.width,
      y: ((event.clientY - rect.top) * canvasSize.value.height) / rect.height,
    }
  }

  const { createRichTextLayer, addTextLayer, addImageLayerFromFile, replaceImageLayerFile, createLayerFromDrag, getBoundsSize, removeLayer, reorderLayer, toggleLayerVisibility, clearLayers } =
    useImageEditorLayers(layers, layerImages, canvasSize, selectedLayerId, activeColor, activeLineWidth, activeFontSize, doc, pushUndo)

  const { createStickerLayer, addStickerLayer, autoResizeSticker, updateStickerText, updateStickerFontSize, toggleStickerBold, toggleStickerItalic } = useImageEditorSticker(
    layers,
    canvasSize,
    selectedLayerId,
    doc,
    pushUndo,
    nextLayerName
  )

  function beginTextDraft(x: number, y: number): AnnotationSessionResult<EditorTextLayer> {
    if (disposed) return { ok: false, error: 'disposed' }
    const draft = createRichTextLayer(x, y, '', '')
    textDraft.value = draft
    previewLayer.value = draft
    selectedLayerId.value = null
    requestRender()
    return { ok: true, value: draft }
  }

  function updateTextDraft(text: string): AnnotationSessionResult<void> {
    if (disposed) return { ok: false, error: 'disposed' }
    if (!textDraft.value) return { ok: true, value: undefined }
    textDraft.value.text = text
    requestRender()
    return { ok: true, value: undefined }
  }

  function cancelTextDraft(): AnnotationSessionResult<void> {
    if (disposed) return { ok: false, error: 'disposed' }
    textDraft.value = null
    previewLayer.value = null
    activeTool.value = 'select'
    requestRender()
    return { ok: true, value: undefined }
  }

  function commitTextDraft(text?: string): AnnotationSessionResult<string | null> {
    if (disposed) return { ok: false, error: 'disposed' }
    const draft = textDraft.value
    if (!draft) return { ok: true, value: null }
    if (text !== undefined) draft.text = text
    if (!draft.text.trim()) {
      cancelTextDraft()
      return { ok: true, value: null }
    }
    pushUndo()
    draft.name = nextLayerName('Text')
    doc.addLayer({ ...draft })
    selectedLayerId.value = draft.id
    textDraft.value = null
    previewLayer.value = null
    activeTool.value = 'select'
    requestRender()
    return { ok: true, value: draft.id }
  }

  function onPointerDown(event: PointerEvent) {
    if (disposed || event.button !== 0) return
    const position = clientToCanvas(event)
    if (!position) return

    if (activeTool.value === 'select') {
      if (transform.beginTransform(position)) return
      for (let index = layers.length - 1; index >= 0; index--) {
        const layer = layers[index]!
        if (layer.visible && hitTestLayer(layer, position.x, position.y)) {
          selectedLayerId.value = layer.id
          beginGesture()
          isDraggingLayer.value = true
          layerDragStart.value = { mx: position.x, my: position.y }
          requestRender()
          return
        }
      }
      selectedLayerId.value = null
      requestRender()
      if (options.allowSourceMove && sourceImg.value) {
        beginGesture()
        isDraggingImage.value = true
        imgDragStart.value = { mx: event.clientX, my: event.clientY, ox: imgOffset.x, oy: imgOffset.y }
      }
      return
    }

    if (crop.cropPointerDown(position, isDrawing, drawStart)) return

    if (activeTool.value === 'text') {
      if ((options.textCommitMode ?? 'draft') === 'draft') {
        beginTextDraft(position.x, position.y)
        return
      }
      pushUndo()
      const layer = createRichTextLayer(position.x, position.y)
      doc.addLayer(layer)
      selectedLayerId.value = layer.id
      activeTool.value = 'select'
      return
    }

    if (activeTool.value === 'sticker') {
      pushUndo()
      const layer = createStickerLayer(position.x, position.y)
      doc.addLayer(layer)
      selectedLayerId.value = layer.id
      activeTool.value = 'select'
      return
    }

    if (activeTool.value === 'brush') {
      isDrawing.value = true
      drawStart.value = position
      previewLayer.value = {
        id: shortUUID(),
        type: 'freehand',
        visible: true,
        name: nextLayerName('Brush'),
        points: [{ x: position.x, y: position.y }],
        color: activeColor.value,
        lineWidth: activeLineWidth.value,
      }
      requestRender()
      return
    }

    isDrawing.value = true
    drawStart.value = position
    selectedLayerId.value = null
    previewLayer.value = createLayerFromDrag(activeTool.value, position, position)
    requestRender()
  }

  function onPointerMove(event: PointerEvent) {
    if (disposed) return
    if (isDraggingImage.value && imgDragStart.value) {
      const wrap = wrapRef.value
      if (!wrap) return
      const scaleX = canvasSize.value.width / wrap.clientWidth
      const scaleY = canvasSize.value.height / wrap.clientHeight
      doc.setImgOffset(imgDragStart.value.ox + (event.clientX - imgDragStart.value.mx) * scaleX, imgDragStart.value.oy + (event.clientY - imgDragStart.value.my) * scaleY)
      return
    }

    const position = clientToCanvas(event)
    if (!position) return
    if (transform.updateTransform(position)) return

    if (isDraggingLayer.value && layerDragStart.value && selectedLayer.value) {
      doc.moveLayerBy(selectedLayer.value.id, position.x - layerDragStart.value.mx, position.y - layerDragStart.value.my)
      layerDragStart.value = { mx: position.x, my: position.y }
      return
    }

    if (activeTool.value === 'select' && !isDraggingLayer.value && !isDrawing.value) {
      overlayCursor.value = transform.getCursorForHandle(position)
      let nextHovered: string | null = null
      for (let index = layers.length - 1; index >= 0; index--) {
        const layer = layers[index]!
        if (layer.visible && hitTestLayer(layer, position.x, position.y)) {
          nextHovered = layer.id
          break
        }
      }
      if (nextHovered !== hoveredLayerId.value) hoveredLayerId.value = nextHovered
    }

    if (activeTool.value === 'crop' && cropRect.value && !isDrawing.value) {
      const cursor = crop.getCropCursor(position)
      if (cursor) overlayCursor.value = cursor
    }
    if (crop.cropPointerMove(position, isDrawing, drawStart)) return
    if (!isDrawing.value || !drawStart.value || !previewLayer.value) return

    if (previewLayer.value.type === 'freehand') {
      ;(previewLayer.value as FreehandLayer).points.push({ x: position.x, y: position.y })
      requestRender()
    } else {
      updateLayerFromDrag(previewLayer.value, drawStart.value, position, event.shiftKey)
      requestRender()
    }
  }

  function onPointerUp() {
    if (disposed) return
    if (isDraggingImage.value) {
      isDraggingImage.value = false
      imgDragStart.value = null
      endGesture()
      return
    }
    if (transform.isActive.value) {
      transform.endTransform()
      endGesture()
      return
    }
    if (isDraggingLayer.value) {
      isDraggingLayer.value = false
      layerDragStart.value = null
      endGesture()
      requestRender()
      return
    }
    if (!isDrawing.value) return
    isDrawing.value = false
    crop.cropPointerUp()
    if (activeTool.value === 'crop') return
    if (previewLayer.value) {
      if (getBoundsSize(previewLayer.value) > 4) {
        pushUndo()
        doc.addLayer(previewLayer.value)
        selectedLayerId.value = previewLayer.value.id
      }
      previewLayer.value = null
    }
  }

  function onPointerCancel() {
    if (disposed) return
    isDrawing.value = false
    drawStart.value = null
    previewLayer.value = null
    isDraggingImage.value = false
    imgDragStart.value = null
    isDraggingLayer.value = false
    layerDragStart.value = null
    transform.endTransform()
    cancelGesture()
    if (textDraft.value) cancelTextDraft()
    if (isCropping.value) crop.cancelCrop()
    requestRender()
  }

  function onDoubleClick(event: MouseEvent) {
    if (disposed) return null
    const position = clientToCanvas(event as unknown as PointerEvent)
    if (!position) return null
    if (crop.onDoubleClickCrop(position)) return null
    for (let index = layers.length - 1; index >= 0; index--) {
      const layer = layers[index]!
      if (layer.visible && hitTestLayer(layer, position.x, position.y) && (isEditorTextLayer(layer) || layer.type === 'sticker')) {
        return layer.id
      }
    }
    return null
  }

  const imageExport = useImageEditorExport(canvasSize, sourceImg, imgOffset, layers, bgColor, layerImages, imgAlpha)

  async function renderPng(selection?: EditorExportRect): Promise<EditorExportResult<Blob> | { ok: false; error: 'disposed' }> {
    if (disposed) return { ok: false, error: 'disposed' }
    const result = await imageExport.renderPng(selection)
    if (disposed) return { ok: false, error: 'disposed' }
    return result
  }

  function preview(): AnnotationSessionResult<void> {
    if (disposed) return { ok: false, error: 'disposed' }
    if (!pixi.isReady) return { ok: false, error: 'preview-not-ready' }
    requestRender()
    return { ok: true, value: undefined }
  }

  async function awaitPaint(): Promise<AnnotationSessionResult<void>> {
    if (disposed) return { ok: false, error: 'disposed' }
    if (!pixi.isReady) return { ok: false, error: 'preview-not-ready' }
    await scheduler.requestMainAndWait()
    if (disposed) return { ok: false, error: 'disposed' }
    if (!pixi.paint()) return { ok: false, error: 'preview-not-ready' }
    return { ok: true, value: undefined }
  }

  function resizeCanvas(width: number, height: number) {
    if (disposed) return
    pushUndo()
    doc.setCanvasSize(width, height)
    requestRender()
  }

  function dispose() {
    if (disposed) return
    disposed = true
    previewGeneration++
    for (const handle of watchHandles.splice(0)) handle.stop()
    scheduler.dispose()
    pixi.destroy()
    releaseCurrentSource()
    sourceImg.value = null
    textDraft.value = null
    canvasRef.value = null
    overlayRef.value = null
    wrapRef.value = null
    binding.dispose()
    undo.dispose()
    doc.destroy()
  }

  return {
    canvasRef,
    overlayRef,
    wrapRef,
    doc,
    binding,
    state: {
      sourceImg,
      imgOffset,
      canvasSize,
      bgColor,
      imgAlpha,
      editorActive,
      isFullscreen,
      inlineEditLayerId,
      renderScale,
      layers,
      selectedLayerId,
      selectedLayer,
      layerImages,
    },
    tools: { activeTool, activeColor, activeLineWidth, activeFontSize, overlayCursor },
    crop: { isCropping, cropRect, applyCrop: crop.applyCrop, cancelCrop: crop.cancelCrop },
    history: { undo: undoFn, redo: redoFn, canUndo, canRedo, pushUndo, clearHistory },
    render: { draw: requestRender, preview, awaitPaint, resizeCanvas },
    source: { set: setSource, clear: clearSource },
    text: { draft: textDraft, begin: beginTextDraft, update: updateTextDraft, commit: commitTextDraft, cancel: cancelTextDraft },
    image: imageApi,
    pointer: { onPointerDown, onPointerMove, onPointerUp, onPointerCancel, onDoubleClick },
    layerOps: {
      clearLayers,
      removeLayer,
      reorderLayer,
      toggleLayerVisibility,
      addTextLayer,
      addStickerLayer,
      addImageLayerFromFile,
      replaceImageLayerFile,
    },
    sticker: {
      autoResizeSticker,
      updateStickerText,
      updateStickerFontSize,
      toggleStickerBold,
      toggleStickerItalic,
    },
    exportOps: { ...imageExport, renderPng },
    dispose,
  }
}

export type AnnotationSession = ReturnType<typeof useAnnotationSession>
