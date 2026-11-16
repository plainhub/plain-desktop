import * as Y from 'yjs'
import type {
  EditorLayer, EditorLayerType,
  ArrowLayer, RectLayer, EllipseLayer, HighlightLayer, MosaicLayer,
  EditorTextLayer, EditorImageLayer, FreehandLayer, StickerLayer,
} from '../utils/types'
import type { TextStroke, TextShadow } from '../utils/text-styles'

export const Y_META_KEY = 'meta'
export const Y_LAYERS_KEY = 'layers'
export const Y_IMAGES_KEY = 'images'
export const DOCUMENT_FORMAT_VERSION = 1

export const META_KEYS = [
  'formatVersion', 'canvasWidth', 'canvasHeight', 'bgColor',
  'imgOffsetX', 'imgOffsetY', 'sourceImage', 'imgAlpha',
] as const

export type MetaKey = typeof META_KEYS[number]

type PointTuple = [number, number]

const LAYER_COMMON_KEYS = ['id', 'type', 'visible', 'name'] as const

export function createYLayer(layer: EditorLayer): Y.Map<unknown> {
  const yMap = new Y.Map<unknown>()
  yMap.set('id', layer.id)
  yMap.set('type', layer.type)
  yMap.set('visible', layer.visible)
  yMap.set('name', layer.name)

  switch (layer.type) {
    case 'arrow': {
      const l = layer as ArrowLayer
      yMap.set('x1', l.x1); yMap.set('y1', l.y1)
      yMap.set('x2', l.x2); yMap.set('y2', l.y2)
      yMap.set('color', l.color); yMap.set('lineWidth', l.lineWidth)
      yMap.set('rotation', l.rotation)
      break
    }
    case 'rect': {
      const l = layer as RectLayer
      yMap.set('x', l.x); yMap.set('y', l.y)
      yMap.set('w', l.w); yMap.set('h', l.h)
      yMap.set('color', l.color); yMap.set('lineWidth', l.lineWidth)
      break
    }
    case 'ellipse': {
      const l = layer as EllipseLayer
      yMap.set('cx', l.cx); yMap.set('cy', l.cy)
      yMap.set('rx', l.rx); yMap.set('ry', l.ry)
      yMap.set('color', l.color); yMap.set('lineWidth', l.lineWidth)
      break
    }
    case 'highlight': {
      const l = layer as HighlightLayer
      yMap.set('x', l.x); yMap.set('y', l.y)
      yMap.set('w', l.w); yMap.set('h', l.h)
      yMap.set('color', l.color)
      break
    }
    case 'mosaic': {
      const l = layer as MosaicLayer
      yMap.set('x', l.x); yMap.set('y', l.y)
      yMap.set('w', l.w); yMap.set('h', l.h)
      yMap.set('blockSize', l.blockSize)
      break
    }
    case 'text': {
      const l = layer as EditorTextLayer
      yMap.set('x', l.x); yMap.set('y', l.y)
      yMap.set('text', l.text)
      yMap.set('fontSize', l.fontSize); yMap.set('color', l.color)
      yMap.set('fontFamily', l.fontFamily)
      yMap.set('fontWeight', l.fontWeight); yMap.set('fontStyle', l.fontStyle)
      yMap.set('textAlign', l.textAlign); yMap.set('letterSpacing', l.letterSpacing)
      yMap.set('textDecoration', l.textDecoration)
      yMap.set('textStroke', createYTextStroke(l.textStroke))
      yMap.set('textShadow', createYTextShadow(l.textShadow))
      yMap.set('glow', l.glow); yMap.set('rotation', l.rotation)
      yMap.set('maxWidth', l.maxWidth)
      break
    }
    case 'image': {
      const l = layer as EditorImageLayer
      yMap.set('x', l.x); yMap.set('y', l.y)
      yMap.set('w', l.w); yMap.set('h', l.h)
      yMap.set('opacity', l.opacity); yMap.set('rotation', l.rotation)
      break
    }
    case 'freehand': {
      const l = layer as FreehandLayer
      const yPoints = new Y.Array<PointTuple>()
      yPoints.push(l.points.map(p => [p.x, p.y] as PointTuple))
      yMap.set('points', yPoints)
      yMap.set('color', l.color); yMap.set('lineWidth', l.lineWidth)
      break
    }
    case 'sticker': {
      const l = layer as StickerLayer
      yMap.set('x', l.x); yMap.set('y', l.y)
      yMap.set('w', l.w); yMap.set('h', l.h)
      yMap.set('color', l.color); yMap.set('text', l.text)
      yMap.set('fontSize', l.fontSize)
      yMap.set('fontWeight', l.fontWeight); yMap.set('fontStyle', l.fontStyle)
      yMap.set('rotation', l.rotation)
      break
    }
  }
  return yMap
}

function createYTextStroke(s: TextStroke): Y.Map<unknown> {
  const m = new Y.Map<unknown>()
  m.set('width', s.width); m.set('color', s.color)
  return m
}

function createYTextShadow(s: TextShadow): Y.Map<unknown> {
  const m = new Y.Map<unknown>()
  m.set('offsetX', s.offsetX); m.set('offsetY', s.offsetY)
  m.set('blur', s.blur); m.set('color', s.color)
  return m
}

function readYTextStroke(m: Y.Map<unknown>): TextStroke {
  return {
    width: m.get('width') as number,
    color: m.get('color') as string,
  }
}

function readYTextShadow(m: Y.Map<unknown>): TextShadow {
  return {
    offsetX: m.get('offsetX') as number,
    offsetY: m.get('offsetY') as number,
    blur: m.get('blur') as number,
    color: m.get('color') as string,
  }
}

export function readYLayer(yMap: Y.Map<unknown>): EditorLayer {
  const type = yMap.get('type') as EditorLayerType
  const id = yMap.get('id') as string
  const visible = yMap.get('visible') as boolean
  const name = yMap.get('name') as string

  switch (type) {
    case 'arrow':
      return {
        id, type, visible, name,
        x1: yMap.get('x1') as number, y1: yMap.get('y1') as number,
        x2: yMap.get('x2') as number, y2: yMap.get('y2') as number,
        color: yMap.get('color') as string,
        lineWidth: yMap.get('lineWidth') as number,
        rotation: yMap.get('rotation') as number,
      }
    case 'rect':
      return {
        id, type, visible, name,
        x: yMap.get('x') as number, y: yMap.get('y') as number,
        w: yMap.get('w') as number, h: yMap.get('h') as number,
        color: yMap.get('color') as string,
        lineWidth: yMap.get('lineWidth') as number,
      }
    case 'ellipse':
      return {
        id, type, visible, name,
        cx: yMap.get('cx') as number, cy: yMap.get('cy') as number,
        rx: yMap.get('rx') as number, ry: yMap.get('ry') as number,
        color: yMap.get('color') as string,
        lineWidth: yMap.get('lineWidth') as number,
      }
    case 'highlight':
      return {
        id, type, visible, name,
        x: yMap.get('x') as number, y: yMap.get('y') as number,
        w: yMap.get('w') as number, h: yMap.get('h') as number,
        color: yMap.get('color') as string,
      }
    case 'mosaic':
      return {
        id, type, visible, name,
        x: yMap.get('x') as number, y: yMap.get('y') as number,
        w: yMap.get('w') as number, h: yMap.get('h') as number,
        blockSize: yMap.get('blockSize') as number,
      }
    case 'text':
      return {
        id, type, visible, name,
        x: yMap.get('x') as number, y: yMap.get('y') as number,
        text: yMap.get('text') as string,
        fontSize: yMap.get('fontSize') as number,
        color: yMap.get('color') as string,
        fontFamily: yMap.get('fontFamily') as string,
        fontWeight: yMap.get('fontWeight') as string,
        fontStyle: yMap.get('fontStyle') as string,
        textAlign: yMap.get('textAlign') as 'left' | 'center' | 'right',
        letterSpacing: yMap.get('letterSpacing') as number,
        textDecoration: yMap.get('textDecoration') as string,
        textStroke: readYTextStroke(yMap.get('textStroke') as Y.Map<unknown>),
        textShadow: readYTextShadow(yMap.get('textShadow') as Y.Map<unknown>),
        glow: yMap.get('glow') as boolean,
        rotation: yMap.get('rotation') as number,
        maxWidth: yMap.get('maxWidth') as number,
      }
    case 'image':
      return {
        id, type, visible, name,
        x: yMap.get('x') as number, y: yMap.get('y') as number,
        w: yMap.get('w') as number, h: yMap.get('h') as number,
        opacity: yMap.get('opacity') as number,
        rotation: yMap.get('rotation') as number,
      }
    case 'freehand': {
      const yPoints = yMap.get('points') as Y.Array<PointTuple>
      const tuples = yPoints.toArray()
      return {
        id, type, visible, name,
        points: tuples.map(t => ({ x: t[0], y: t[1] })),
        color: yMap.get('color') as string,
        lineWidth: yMap.get('lineWidth') as number,
      }
    }
    case 'sticker':
      return {
        id, type, visible, name,
        x: yMap.get('x') as number, y: yMap.get('y') as number,
        w: yMap.get('w') as number, h: yMap.get('h') as number,
        color: yMap.get('color') as string,
        text: yMap.get('text') as string,
        fontSize: yMap.get('fontSize') as number,
        fontWeight: yMap.get('fontWeight') as string,
        fontStyle: yMap.get('fontStyle') as string,
        rotation: yMap.get('rotation') as number,
      }
    default:
      throw new Error('Unsupported image editor layer type')
  }
}

export function moveYLayer(yMap: Y.Map<unknown>, dx: number, dy: number): void {
  const type = yMap.get('type') as EditorLayerType
  switch (type) {
    case 'arrow':
      yMap.set('x1', (yMap.get('x1') as number) + dx)
      yMap.set('y1', (yMap.get('y1') as number) + dy)
      yMap.set('x2', (yMap.get('x2') as number) + dx)
      yMap.set('y2', (yMap.get('y2') as number) + dy)
      break
    case 'rect': case 'highlight': case 'mosaic':
      yMap.set('x', (yMap.get('x') as number) + dx)
      yMap.set('y', (yMap.get('y') as number) + dy)
      break
    case 'ellipse':
      yMap.set('cx', (yMap.get('cx') as number) + dx)
      yMap.set('cy', (yMap.get('cy') as number) + dy)
      break
    case 'text': case 'image': case 'sticker':
      yMap.set('x', (yMap.get('x') as number) + dx)
      yMap.set('y', (yMap.get('y') as number) + dy)
      break
    case 'freehand': {
      const yPoints = yMap.get('points') as Y.Array<PointTuple> | undefined
      if (!yPoints) break
      const moved = yPoints.toArray().map(t => [t[0] + dx, t[1] + dy] as PointTuple)
      const newYPoints = new Y.Array<PointTuple>()
      newYPoints.push(moved)
      yMap.set('points', newYPoints)
      break
    }
  }
}

export function appendFreehandPoint(yMap: Y.Map<unknown>, x: number, y: number): void {
  const yPoints = yMap.get('points') as Y.Array<PointTuple> | undefined
  if (!yPoints) return
  yPoints.push([[x, y] as PointTuple])
}

export function getLayerCommonKeys(): readonly string[] {
  return LAYER_COMMON_KEYS
}
