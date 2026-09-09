import { ref, type Ref } from 'vue'
import type { EditorLayer, CanvasSize, EditorRasterSource } from '@/views/image-editor/utils/types'
import { renderEditorCanvas } from '@/views/image-editor/utils/renderer'
import type { ImageEditorDoc } from './useImageEditorDoc'

export function useEditorCrop(
  canvasSize: Ref<CanvasSize>,
  sourceImg: Ref<EditorRasterSource | null>,
  imgOffset: { x: number; y: number },
  layers: EditorLayer[],
  bgColor: Ref<string>,
  layerImages: Map<string, HTMLImageElement>,
  imgAlpha: Ref<number>,
  doc: ImageEditorDoc,
  pushUndo: () => void,
  activeTool: Ref<string>,
  requestRender: () => void,
  persistSourceImage?: (value: string | null) => void,
  enabled = true,
) {
  const isCropping = ref(false)
  const cropRect = ref<{ x: number; y: number; w: number; h: number } | null>(null)
  const cropDragMode = ref<string | null>(null)
  const cropDragStart = ref<{ mx: number; my: number; rect: { x: number; y: number; w: number; h: number } } | null>(null)

  function hitTestCropHandle(px: number, py: number): string | null {
    const r = cropRect.value
    if (!r || r.w < 2 || r.h < 2) return null
    const tol = 18
    if (Math.hypot(px - r.x, py - r.y) < tol) return 'nw'
    if (Math.hypot(px - (r.x + r.w), py - r.y) < tol) return 'ne'
    if (Math.hypot(px - r.x, py - (r.y + r.h)) < tol) return 'sw'
    if (Math.hypot(px - (r.x + r.w), py - (r.y + r.h)) < tol) return 'se'
    if (Math.hypot(px - (r.x + r.w / 2), py - r.y) < tol) return 'n'
    if (Math.hypot(px - (r.x + r.w / 2), py - (r.y + r.h)) < tol) return 's'
    if (Math.hypot(px - r.x, py - (r.y + r.h / 2)) < tol) return 'w'
    if (Math.hypot(px - (r.x + r.w), py - (r.y + r.h / 2)) < tol) return 'e'
    if (px >= r.x && px <= r.x + r.w && py >= r.y && py <= r.y + r.h) return 'move'
    return null
  }

  function drawCropOverlay(ctx: CanvasRenderingContext2D, rect: { x: number; y: number; w: number; h: number }) {
    const { width: cw, height: ch } = canvasSize.value
    ctx.save()
    ctx.fillStyle = 'rgba(0,0,0,0.5)'
    ctx.fillRect(0, 0, cw, ch)
    ctx.clearRect(rect.x, rect.y, rect.w, rect.h)
    ctx.beginPath(); ctx.rect(rect.x, rect.y, rect.w, rect.h); ctx.clip()
    renderEditorCanvas(ctx, sourceImg.value, imgOffset, layers, canvasSize.value, bgColor.value, null, layerImages, undefined, imgAlpha.value)
    ctx.restore()
    ctx.save()
    ctx.strokeStyle = '#3b82f6'; ctx.lineWidth = 2; ctx.setLineDash([6, 4])
    ctx.strokeRect(rect.x, rect.y, rect.w, rect.h)
    ctx.setLineDash([])
    const hs = 8
    const handles = [
      [rect.x, rect.y], [rect.x + rect.w, rect.y],
      [rect.x, rect.y + rect.h], [rect.x + rect.w, rect.y + rect.h],
      [rect.x + rect.w / 2, rect.y], [rect.x + rect.w / 2, rect.y + rect.h],
      [rect.x, rect.y + rect.h / 2], [rect.x + rect.w, rect.y + rect.h / 2],
    ]
    ctx.fillStyle = '#fff'; ctx.strokeStyle = '#3b82f6'; ctx.lineWidth = 2
    for (const [hx, hy] of handles) {
      ctx.fillRect(hx! - hs / 2, hy! - hs / 2, hs, hs)
      ctx.strokeRect(hx! - hs / 2, hy! - hs / 2, hs, hs)
    }
    ctx.restore()
  }

  function cropPointerDown(pos: { x: number; y: number }, isDrawing: Ref<boolean>, drawStart: Ref<{ x: number; y: number } | null>): boolean {
    if (!enabled || activeTool.value !== 'crop') return false
    const { width: cw, height: ch } = canvasSize.value
    if (cropRect.value && cropRect.value.w > 2 && cropRect.value.h > 2) {
      if (pos.x < 0 || pos.x > cw || pos.y < 0 || pos.y > ch) {
        applyCrop()
        return true
      }
      const mode = hitTestCropHandle(pos.x, pos.y)
      if (mode) {
        cropDragMode.value = mode
        cropDragStart.value = { mx: pos.x, my: pos.y, rect: { ...cropRect.value } }
        isDrawing.value = true
        return true
      }
    }
    isCropping.value = true
    cropRect.value = { x: pos.x, y: pos.y, w: 0, h: 0 }
    cropDragMode.value = null
    isDrawing.value = true; drawStart.value = pos
    return true
  }

  function cropPointerMove(pos: { x: number; y: number }, isDrawing: Ref<boolean>, drawStart: Ref<{ x: number; y: number } | null>): boolean {
    if (!enabled) return false
    if (activeTool.value === 'crop' && cropRect.value && !isDrawing.value) {
      return false
    }
    if (isDrawing.value && cropDragMode.value && cropDragStart.value && cropRect.value) {
      const dx = pos.x - cropDragStart.value.mx
      const dy = pos.y - cropDragStart.value.my
      const r = cropDragStart.value.rect
      const { width: cw, height: ch } = canvasSize.value
      const mode = cropDragMode.value
      if (mode === 'move') {
        cropRect.value = {
          x: Math.max(0, Math.min(cw - r.w, r.x + dx)),
          y: Math.max(0, Math.min(ch - r.h, r.y + dy)),
          w: r.w, h: r.h,
        }
      } else {
        let { x, y, w, h } = r
        if (mode.includes('w')) { x = Math.max(0, r.x + dx); w = r.w - dx }
        if (mode.includes('e')) { w = r.w + dx }
        if (mode.includes('n')) { y = Math.max(0, r.y + dy); h = r.h - dy }
        if (mode.includes('s')) { h = r.h + dy }
        if (x < 0) { w += x; x = 0 }
        if (y < 0) { h += y; y = 0 }
        w = Math.max(10, Math.min(cw - x, w))
        h = Math.max(10, Math.min(ch - y, h))
        cropRect.value = { x, y, w, h }
      }
      requestRender(); return true
    }
    if (isDrawing.value && drawStart.value && activeTool.value === 'crop' && cropRect.value) {
      const { width: cw, height: ch } = canvasSize.value
      const cx = Math.max(0, Math.min(cw, pos.x))
      const cy = Math.max(0, Math.min(ch, pos.y))
      cropRect.value = {
        x: Math.min(drawStart.value.x, cx), y: Math.min(drawStart.value.y, cy),
        w: Math.abs(cx - drawStart.value.x), h: Math.abs(cy - drawStart.value.y),
      }
      requestRender(); return true
    }
    return false
  }

  function cropPointerUp(): boolean {
    if (!enabled || (activeTool.value !== 'crop' && !cropDragMode.value)) return false
    cropDragMode.value = null; cropDragStart.value = null
    return false
  }

  function getCropCursor(pos: { x: number; y: number }): string | null {
    if (!enabled || activeTool.value !== 'crop' || !cropRect.value) return null
    const mode = hitTestCropHandle(pos.x, pos.y)
    if (mode === 'move') return 'default'
    if (mode === 'nw' || mode === 'se') return 'nwse-resize'
    if (mode === 'ne' || mode === 'sw') return 'nesw-resize'
    if (mode === 'n' || mode === 's') return 'ns-resize'
    if (mode === 'e' || mode === 'w') return 'ew-resize'
    return 'crosshair'
  }

  function applyCrop() {
    if (!enabled || !cropRect.value) return
    const r = cropRect.value
    if (r.w < 2 || r.h < 2) { cancelCrop(); return }
    const rw = Math.round(r.w), rh = Math.round(r.h)
    const tmp = document.createElement('canvas')
    tmp.width = rw; tmp.height = rh
    const tctx = tmp.getContext('2d')!
    tctx.translate(-Math.round(r.x), -Math.round(r.y))
    renderEditorCanvas(tctx, sourceImg.value, imgOffset, layers, canvasSize.value, bgColor.value, null, layerImages, undefined, imgAlpha.value)
    const dataUrl = tmp.toDataURL('image/png')
    const img = new Image()
    img.onload = () => {
      pushUndo()
      sourceImg.value = img
      doc.ydoc.transact(() => {
        persistSourceImage?.(dataUrl)
        doc.setCanvasSize(rw, rh)
        doc.setImgOffset(0, 0)
        doc.clearLayers()
      })
      isCropping.value = false; cropRect.value = null; activeTool.value = 'select'
      requestRender()
    }
    img.src = dataUrl
  }

  function cancelCrop() {
    isCropping.value = false; cropRect.value = null; cropDragMode.value = null; cropDragStart.value = null
    activeTool.value = 'select'; requestRender()
  }

  function drawCropIfActive(ctx: CanvasRenderingContext2D) {
    if (enabled && isCropping.value && cropRect.value && cropRect.value.w > 1 && cropRect.value.h > 1) drawCropOverlay(ctx, cropRect.value)
  }

  function onDoubleClickCrop(pos: { x: number; y: number }): boolean {
    if (!enabled || !isCropping.value || !cropRect.value) return false
    const r = cropRect.value
    if (r.w > 2 && r.h > 2 && pos.x >= r.x && pos.x <= r.x + r.w && pos.y >= r.y && pos.y <= r.y + r.h) {
      applyCrop()
      return true
    }
    return false
  }

  return {
    isCropping, cropRect,
    cropPointerDown, cropPointerMove, cropPointerUp,
    getCropCursor, drawCropIfActive, onDoubleClickCrop,
    applyCrop, cancelCrop,
  }
}
