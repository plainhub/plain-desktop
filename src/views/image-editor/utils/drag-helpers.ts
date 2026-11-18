/**
 * Pure geometry helpers for layer drag operations.
 * Extracted from useImageEditorCore for testability.
 */
import { EditorLayerType, type EditorLayer, type RectLayer, type HighlightLayer, type MosaicLayer, type EllipseLayer } from './types'

export function updateLayerFromDrag(
  layer: EditorLayer,
  start: { x: number; y: number },
  end: { x: number; y: number },
  shiftKey = false,
) {
  switch (layer.type) {
    case EditorLayerType.ARROW: layer.x2 = end.x; layer.y2 = end.y; break
    case EditorLayerType.RECT: case EditorLayerType.HIGHLIGHT: case EditorLayerType.MOSAIC: {
      const l = layer as RectLayer | HighlightLayer | MosaicLayer
      let w = Math.abs(end.x - start.x)
      let h = Math.abs(end.y - start.y)
      if (shiftKey) { const s = Math.max(w, h); w = s; h = s }
      l.x = end.x >= start.x ? start.x : start.x - w
      l.y = end.y >= start.y ? start.y : start.y - h
      l.w = w; l.h = h; break
    }
    case EditorLayerType.ELLIPSE: {
      let rw = Math.abs(end.x - start.x) / 2
      let rh = Math.abs(end.y - start.y) / 2
      if (shiftKey) { const r = Math.max(rw, rh); rw = r; rh = r }
      const hw = end.x >= start.x ? rw : -rw
      const hh = end.y >= start.y ? rh : -rh
      layer.cx = start.x + hw; layer.cy = start.y + hh
      layer.rx = rw; layer.ry = rh; break
    }
  }
}
