import { ref, computed, type Ref } from 'vue'
import type {
  EditorLayer, EditorTextLayer, EditorImageLayer,
  ArrowLayer, EllipseLayer,
} from '@/views/image-editor/utils/types'
import { getSelectionLayout } from '@/views/image-editor/utils/renderer'
import type { ImageEditorDoc } from './useImageEditorDoc'

function unrotatePoint(px: number, py: number, cx: number, cy: number, deg: number) {
  if (!deg) return { x: px, y: py }
  const rad = -(deg * Math.PI) / 180
  const dx = px - cx, dy = py - cy
  return { x: cx + dx * Math.cos(rad) - dy * Math.sin(rad), y: cy + dx * Math.sin(rad) + dy * Math.cos(rad) }
}

export function useEditorTransform(
  layers: EditorLayer[],
  selectedLayerId: Ref<string | null>,
  doc: ImageEditorDoc,
  beginGesture: () => void,
) {
  const isRotating = ref(false)
  const isScaling = ref(false)
  const isWidthDragging = ref(false)
  const overlayCursor = ref('default')

  let rotateStartAngle = 0
  let scaleStartDist = 0, scaleStartW = 0, scaleStartH = 0
  let scaleStartCx = 0, scaleStartCy = 0
  let scaleStartFontSize = 0, scaleStartMaxWidth = 0
  let scaleStartX1 = 0, scaleStartY1 = 0, scaleStartX2 = 0, scaleStartY2 = 0
  let widthDragSide: 'left' | 'right' = 'right'
  let widthDragStartX = 0, widthDragStartW = 0, widthDragStartH = 0, widthDragStartMaxWidth = 0

  const isActive = computed(() => isRotating.value || isScaling.value || isWidthDragging.value)

  function hitTestHandle(cx2: number, cy2: number): string | null {
    if (!selectedLayerId.value) return null
    const sel = layers.find(l => l.id === selectedLayerId.value)
    if (!sel) return null
    const { bounds, pad, cx, cy, rotation } = getSelectionLayout(sel)
    const p = unrotatePoint(cx2, cy2, cx, cy, rotation)
    const hasRotation = sel.type === 'text' || sel.type === 'image' || sel.type === 'arrow' || sel.type === 'sticker'
    if (hasRotation) {
      const handleX = bounds.x + bounds.w / 2
      const handleY = bounds.y - pad - 36
      if (Math.hypot(p.x - handleX, p.y - handleY) < 25) return 'rotate'
    }
    const cornerDefs: [number, number, string][] = [
      [bounds.x - pad, bounds.y - pad, 'scale-nw'],
      [bounds.x + bounds.w + pad, bounds.y - pad, 'scale-ne'],
      [bounds.x - pad, bounds.y + bounds.h + pad, 'scale-sw'],
      [bounds.x + bounds.w + pad, bounds.y + bounds.h + pad, 'scale-se'],
    ]
    for (const [hx, hy, tag] of cornerDefs) {
      if (Math.hypot(p.x - hx, p.y - hy) < 18) return tag
    }
    if (sel.type === 'text' || sel.type === 'image') {
      const edgeMidY = bounds.y + bounds.h / 2
      if (Math.hypot(p.x - (bounds.x - pad), p.y - edgeMidY) < 20) return 'edge-left'
      if (Math.hypot(p.x - (bounds.x + bounds.w + pad), p.y - edgeMidY) < 20) return 'edge-right'
    }
    return null
  }

  function beginTransform(pos: { x: number; y: number }): boolean {
    const handle = hitTestHandle(pos.x, pos.y)
    if (!handle || !selectedLayerId.value) return false
    const sel = layers.find(l => l.id === selectedLayerId.value)!
    const { cx, cy } = getSelectionLayout(sel)

    if (handle === 'rotate') {
      beginGesture()
      isRotating.value = true
      const rot = ('rotation' in sel) ? (sel as any).rotation ?? 0 : 0
      rotateStartAngle = Math.atan2(pos.y - cy, pos.x - cx) - (rot * Math.PI) / 180
      return true
    }
    if (handle.startsWith('scale-')) {
      beginGesture()
      isScaling.value = true
      scaleStartCx = cx; scaleStartCy = cy
      scaleStartDist = Math.hypot(pos.x - cx, pos.y - cy)
      if (sel.type === 'image') { scaleStartW = (sel as EditorImageLayer).w; scaleStartH = (sel as EditorImageLayer).h }
      else if (sel.type === 'text') { scaleStartFontSize = (sel as EditorTextLayer).fontSize; scaleStartMaxWidth = (sel as EditorTextLayer).maxWidth }
      else if (sel.type === 'arrow') { const a = sel as ArrowLayer; scaleStartX1 = a.x1; scaleStartY1 = a.y1; scaleStartX2 = a.x2; scaleStartY2 = a.y2 }
      else if ('w' in sel && 'h' in sel) { scaleStartW = (sel as any).w; scaleStartH = (sel as any).h }
      else if ('rx' in sel) { scaleStartW = sel.rx; scaleStartH = sel.ry }
      return true
    }
    if (handle === 'edge-left' || handle === 'edge-right') {
      beginGesture()
      isWidthDragging.value = true
      widthDragSide = handle === 'edge-left' ? 'left' : 'right'
      widthDragStartX = pos.x
      if (sel.type === 'image') { widthDragStartW = (sel as EditorImageLayer).w; widthDragStartH = (sel as EditorImageLayer).h }
      else if (sel.type === 'text') { widthDragStartMaxWidth = (sel as EditorTextLayer).maxWidth }
      return true
    }
    return false
  }

  function updateTransform(pos: { x: number; y: number }): boolean {
    if (!selectedLayerId.value) return false
    const id = selectedLayerId.value

    if (isRotating.value) {
      const sel = layers.find(l => l.id === id)
      if (sel && 'rotation' in sel) {
        const { cx, cy } = getSelectionLayout(sel)
        let deg = ((Math.atan2(pos.y - cy, pos.x - cx) - rotateStartAngle) * 180) / Math.PI
        for (const snap of [0, 90, 180, 270, -90, -180, -270, 360]) {
          if (Math.abs(deg - snap) < 3) { deg = snap; break }
        }
        doc.setLayerProp(id, 'rotation', Math.round(deg % 360))
        return true
      }
    }
    if (isScaling.value) {
      const sel = layers.find(l => l.id === id)
      if (sel && scaleStartDist > 0) {
        const ratio = Math.hypot(pos.x - scaleStartCx, pos.y - scaleStartCy) / scaleStartDist
        _applyScale(id, sel, ratio)
        return true
      }
    }
    if (isWidthDragging.value) {
      const sel = layers.find(l => l.id === id)
      if (sel) {
        const delta = pos.x - widthDragStartX
        const sign = widthDragSide === 'right' ? 1 : -1
        if (sel.type === 'image') {
          const ar = widthDragStartH / widthDragStartW
          const newW = Math.max(20, Math.round(widthDragStartW + delta * sign))
          const newH = Math.max(20, Math.round(newW * ar))
          doc.setLayerProps(id, { w: newW, h: newH })
        } else if (sel.type === 'text') {
          doc.setLayerProp(id, 'maxWidth', Math.max(50, Math.round(widthDragStartMaxWidth + delta * sign)))
        }
        return true
      }
    }
    return false
  }

  function _applyScale(id: string, sel: EditorLayer, ratio: number) {
    if (sel.type === 'image') {
      doc.setLayerProps(id, {
        w: Math.max(20, Math.round(scaleStartW * ratio)),
        h: Math.max(20, Math.round(scaleStartH * ratio)),
      })
    } else if (sel.type === 'text') {
      doc.setLayerProps(id, {
        fontSize: Math.max(10, Math.min(500, Math.round(scaleStartFontSize * ratio))),
        maxWidth: Math.max(50, Math.round(scaleStartMaxWidth * ratio)),
      })
    } else if (sel.type === 'arrow') {
      const sCx = (scaleStartX1 + scaleStartX2) / 2
      const sCy = (scaleStartY1 + scaleStartY2) / 2
      doc.setLayerProps(id, {
        x1: Math.round(sCx + (scaleStartX1 - sCx) * ratio),
        y1: Math.round(sCy + (scaleStartY1 - sCy) * ratio),
        x2: Math.round(sCx + (scaleStartX2 - sCx) * ratio),
        y2: Math.round(sCy + (scaleStartY2 - sCy) * ratio),
      })
    } else if ('w' in sel && 'h' in sel) {
      const nw = Math.max(4, Math.round(scaleStartW * ratio))
      const nh = Math.max(4, Math.round(scaleStartH * ratio))
      const props: Record<string, unknown> = { w: nw, h: nh }
      if (sel.type === 'rect' || sel.type === 'highlight' || sel.type === 'mosaic') {
        props.x = Math.round(scaleStartCx - nw / 2)
        props.y = Math.round(scaleStartCy - nh / 2)
      }
      doc.setLayerProps(id, props)
    } else if ('rx' in sel) {
      doc.setLayerProps(id, {
        rx: Math.max(2, Math.round(scaleStartW * ratio)),
        ry: Math.max(2, Math.round(scaleStartH * ratio)),
      })
    }
  }

  function endTransform() {
    isRotating.value = false
    isScaling.value = false
    isWidthDragging.value = false
  }

  function getCursorForHandle(pos: { x: number; y: number }): string {
    const handle = hitTestHandle(pos.x, pos.y)
    if (handle === 'rotate') return 'grab'
    if (handle === 'scale-nw' || handle === 'scale-se') return 'nwse-resize'
    if (handle === 'scale-ne' || handle === 'scale-sw') return 'nesw-resize'
    if (handle === 'edge-left' || handle === 'edge-right') return 'ew-resize'
    return 'default'
  }

  return {
    isActive, overlayCursor,
    hitTestHandle, beginTransform, updateTransform, endTransform, getCursorForHandle,
  }
}
