import type { TextStroke, TextShadow } from '@/views/image-editor/utils/text-styles'

/** Tool types available in the image editor */
export type EditorTool =
  | 'select'
  | 'crop'
  | 'arrow'
  | 'rect'
  | 'ellipse'
  | 'highlight'
  | 'mosaic'
  | 'text'
  | 'brush'
  | 'sticker'

/** All possible layer discriminator values */
export type EditorLayerType = 'arrow' | 'rect' | 'ellipse' | 'highlight' | 'mosaic' | 'text' | 'image' | 'freehand' | 'sticker'

/** Base for every layer type */
interface LayerBase {
  id: string
  type: EditorLayerType
  visible: boolean
  name: string
}

export interface ArrowLayer extends LayerBase {
  type: 'arrow'
  x1: number; y1: number
  x2: number; y2: number
  color: string
  lineWidth: number
  rotation: number
}

export interface RectLayer extends LayerBase {
  type: 'rect'
  x: number; y: number
  w: number; h: number
  color: string
  lineWidth: number
}

export interface EllipseLayer extends LayerBase {
  type: 'ellipse'
  cx: number; cy: number
  rx: number; ry: number
  color: string
  lineWidth: number
}

export interface HighlightLayer extends LayerBase {
  type: 'highlight'
  x: number; y: number
  w: number; h: number
  color: string
}

export interface MosaicLayer extends LayerBase {
  type: 'mosaic'
  x: number; y: number
  w: number; h: number
  blockSize: number
}

/** Rich text layer with font styling, stroke, shadow, glow */
export interface EditorTextLayer extends LayerBase {
  type: 'text'
  x: number; y: number
  text: string
  fontSize: number
  color: string
  fontFamily: string
  fontWeight: string
  fontStyle: string
  textAlign: 'left' | 'center' | 'right'
  letterSpacing: number
  textDecoration: string
  textStroke: TextStroke
  textShadow: TextShadow
  glow: boolean
  rotation: number
  maxWidth: number
}

/** Image layer (actual HTMLImageElement stored in layerImages Map) */
export interface EditorImageLayer extends LayerBase {
  type: 'image'
  x: number; y: number
  w: number; h: number
  opacity: number
  rotation: number
}

/** Freehand brush stroke layer */
export interface FreehandLayer extends LayerBase {
  type: 'freehand'
  points: { x: number; y: number }[]
  color: string
  lineWidth: number
}

/** Sticky note layer */
export interface StickerLayer extends LayerBase {
  type: 'sticker'
  x: number; y: number
  w: number; h: number
  color: string
  text: string
  fontSize: number
  fontWeight: string
  fontStyle: string
  rotation: number
}

export type EditorLayer =
  | ArrowLayer
  | RectLayer
  | EllipseLayer
  | HighlightLayer
  | MosaicLayer
  | EditorTextLayer
  | EditorImageLayer
  | FreehandLayer
  | StickerLayer

export interface CanvasSize {
  width: number
  height: number
}

/** Raster sources accepted by the in-memory editor without serialization. */
export type EditorRasterSource = HTMLImageElement | HTMLCanvasElement

export function getEditorRasterSize(source: EditorRasterSource): CanvasSize {
  if ('naturalWidth' in source) {
    return { width: source.naturalWidth, height: source.naturalHeight }
  }
  return { width: source.width, height: source.height }
}

export interface SizePreset {
  label: string
  w: number
  h: number
}

export const SIZE_PRESETS: SizePreset[] = [
  { label: 'HD (1920×1080)', w: 1920, h: 1080 },
  { label: '4K (3840×2160)', w: 3840, h: 2160 },
]

export const TOOL_COLORS = [
  '#ef4444', '#f97316', '#eab308', '#22c55e',
  '#3b82f6', '#8b5cf6', '#000000', '#ffffff',
]

export const LINE_WIDTHS = [2, 4, 8, 16]

export interface ToolMeta {
  id: EditorTool
  icon: string
  labelKey: string
}

export const TOOLS: ToolMeta[] = [
  { id: 'select', icon: 'ph:cursor', labelKey: 'imageEditor.toolSelect' },
  { id: 'crop', icon: 'ph:crop', labelKey: 'imageEditor.toolCrop' },
  { id: 'brush', icon: 'ph:paint-brush', labelKey: 'imageEditor.toolBrush' },
  { id: 'arrow', icon: 'ph:arrow-up-right', labelKey: 'imageEditor.toolArrow' },
  { id: 'rect', icon: 'ph:rectangle', labelKey: 'imageEditor.toolRect' },
  { id: 'ellipse', icon: 'ph:circle', labelKey: 'imageEditor.toolEllipse' },
  { id: 'highlight', icon: 'ph:highlighter-circle', labelKey: 'imageEditor.toolHighlight' },
  { id: 'mosaic', icon: 'ph:grid-nine', labelKey: 'imageEditor.toolMosaic' },
  { id: 'text', icon: 'ph:text-t', labelKey: 'imageEditor.toolText' },
  { id: 'sticker', icon: 'ph:note', labelKey: 'imageEditor.toolSticker' },
]

export function isEditorTextLayer(layer: EditorLayer): layer is EditorTextLayer {
  return layer.type === 'text'
}

export function isEditorImageLayer(layer: EditorLayer): layer is EditorImageLayer {
  return layer.type === 'image'
}

const HIT_TOL = 8

function unrotatePoint(px: number, py: number, cx: number, cy: number, deg: number) {
  if (!deg) return { x: px, y: py }
  const rad = -(deg * Math.PI) / 180
  const dx = px - cx, dy = py - cy
  return { x: cx + dx * Math.cos(rad) - dy * Math.sin(rad), y: cy + dx * Math.sin(rad) + dy * Math.cos(rad) }
}

export function hitTestLayer(layer: EditorLayer, px: number, py: number): boolean {
  switch (layer.type) {
    case 'arrow': {
      const cx = (layer.x1 + layer.x2) / 2, cy = (layer.y1 + layer.y2) / 2
      const p = unrotatePoint(px, py, cx, cy, layer.rotation)
      return distToSeg(p.x, p.y, layer.x1, layer.y1, layer.x2, layer.y2) < HIT_TOL + layer.lineWidth
    }
    case 'rect':
    case 'highlight':
    case 'mosaic': {
      const l = layer as RectLayer | HighlightLayer | MosaicLayer
      return px >= l.x - HIT_TOL && px <= l.x + l.w + HIT_TOL
        && py >= l.y - HIT_TOL && py <= l.y + l.h + HIT_TOL
    }
    case 'ellipse': {
      const dx = (px - layer.cx) / (layer.rx + HIT_TOL)
      const dy = (py - layer.cy) / (layer.ry + HIT_TOL)
      return dx * dx + dy * dy <= 1
    }
    case 'text': {
      const l = layer as EditorTextLayer
      const halfW = l.maxWidth / 2
      const charW = l.fontSize * 0.6
      const lineCount = Math.max(1, Math.ceil((l.text.length * charW) / l.maxWidth))
      const halfH = (lineCount * l.fontSize * 1.15) / 2
      const p = unrotatePoint(px, py, l.x, l.y, l.rotation)
      return p.x >= l.x - halfW - HIT_TOL && p.x <= l.x + halfW + HIT_TOL
        && p.y >= l.y - halfH - HIT_TOL && p.y <= l.y + halfH + HIT_TOL
    }
    case 'image': {
      const l = layer as EditorImageLayer
      const p = unrotatePoint(px, py, l.x, l.y, l.rotation)
      return p.x >= l.x - l.w / 2 - HIT_TOL && p.x <= l.x + l.w / 2 + HIT_TOL
        && p.y >= l.y - l.h / 2 - HIT_TOL && p.y <= l.y + l.h / 2 + HIT_TOL
    }
    case 'freehand': {
      for (const pt of layer.points) {
        if (Math.hypot(px - pt.x, py - pt.y) < HIT_TOL + layer.lineWidth) return true
      }
      return false
    }
    case 'sticker': {
      const l = layer as StickerLayer
      const p = unrotatePoint(px, py, l.x, l.y, l.rotation)
      return p.x >= l.x - l.w / 2 - HIT_TOL && p.x <= l.x + l.w / 2 + HIT_TOL
        && p.y >= l.y - l.h / 2 - HIT_TOL && p.y <= l.y + l.h / 2 + HIT_TOL
    }
  }
}

function distToSeg(px: number, py: number, x1: number, y1: number, x2: number, y2: number) {
  const dx = x2 - x1, dy = y2 - y1
  const lenSq = dx * dx + dy * dy
  if (lenSq === 0) return Math.hypot(px - x1, py - y1)
  const t = Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / lenSq))
  return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy))
}

export function getEditorLayerBounds(layer: EditorLayer): { x: number; y: number; w: number; h: number } {
  switch (layer.type) {
    case 'arrow':
      return { x: Math.min(layer.x1, layer.x2), y: Math.min(layer.y1, layer.y2), w: Math.abs(layer.x2 - layer.x1), h: Math.abs(layer.y2 - layer.y1) }
    case 'rect': case 'highlight': case 'mosaic': {
      const l = layer as RectLayer | HighlightLayer | MosaicLayer
      return { x: l.x, y: l.y, w: l.w, h: l.h }
    }
    case 'ellipse':
      return { x: layer.cx - layer.rx, y: layer.cy - layer.ry, w: layer.rx * 2, h: layer.ry * 2 }
    case 'text': {
      const l = layer as EditorTextLayer
      const charW = l.fontSize * 0.6
      const lineCount = Math.max(1, Math.ceil((l.text.length * charW) / l.maxWidth))
      const h = lineCount * l.fontSize * 1.15
      return { x: l.x - l.maxWidth / 2, y: l.y - h / 2, w: l.maxWidth, h }
    }
    case 'image': {
      const l = layer as EditorImageLayer
      return { x: l.x - l.w / 2, y: l.y - l.h / 2, w: l.w, h: l.h }
    }
    case 'freehand': {
      if (!layer.points.length) return { x: 0, y: 0, w: 0, h: 0 }
      let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
      for (const p of layer.points) {
        if (p.x < minX) minX = p.x
        if (p.y < minY) minY = p.y
        if (p.x > maxX) maxX = p.x
        if (p.y > maxY) maxY = p.y
      }
      return { x: minX, y: minY, w: maxX - minX, h: maxY - minY }
    }
    case 'sticker': {
      const l = layer as StickerLayer
      return { x: l.x - l.w / 2, y: l.y - l.h / 2, w: l.w, h: l.h }
    }
  }
}

export function moveLayerBy(layer: EditorLayer, dx: number, dy: number) {
  switch (layer.type) {
    case 'arrow':
      layer.x1 += dx; layer.y1 += dy; layer.x2 += dx; layer.y2 += dy; break
    case 'rect': case 'highlight': case 'mosaic':
      (layer as RectLayer | HighlightLayer | MosaicLayer).x += dx
      ;(layer as RectLayer | HighlightLayer | MosaicLayer).y += dy
      break
    case 'ellipse':
      layer.cx += dx; layer.cy += dy; break
    case 'text': case 'image': case 'sticker':
      (layer as EditorTextLayer | EditorImageLayer | StickerLayer).x += dx
      ;(layer as EditorTextLayer | EditorImageLayer | StickerLayer).y += dy
      break
    case 'freehand':
      for (const p of layer.points) { p.x += dx; p.y += dy }
      break
  }
}
