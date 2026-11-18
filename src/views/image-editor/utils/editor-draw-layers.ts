import { cssFontWeight } from '@/views/image-editor/utils/text-styles'
/**
 * Individual layer draw functions for the image editor canvas.
 * Called exclusively by `drawLayer()` dispatcher.
 */

import {
  EditorLayerType, EditorTextAlign, type EditorLayer, type ArrowLayer, type RectLayer, type EllipseLayer,
  type HighlightLayer, type EditorTextLayer, type EditorImageLayer, type FreehandLayer, type StickerLayer,
} from '@/views/image-editor/utils/types'

export function wrapText(ctx: CanvasRenderingContext2D, text: string, maxWidth: number): string[] {
  if (!text) return ['']
  const lines: string[] = []
  for (const paragraph of text.split('\n')) {
    if (!paragraph) { lines.push(''); continue }
    const words = paragraph.split(' ')
    let cur = ''
    for (const w of words) {
      const test = cur ? `${cur} ${w}` : w
      if (ctx.measureText(test).width > maxWidth && cur) {
        lines.push(cur)
        cur = w
      } else {
        cur = test
      }
      while (ctx.measureText(cur).width > maxWidth && cur.length > 1) {
        let split = cur.length - 1
        while (split > 0 && ctx.measureText(cur.slice(0, split)).width > maxWidth) split--
        if (split <= 0) split = 1
        lines.push(cur.slice(0, split))
        cur = cur.slice(split)
      }
    }
    if (cur) lines.push(cur)
    else if (!lines.length) lines.push('')
  }
  return lines.length ? lines : ['']
}

export function drawLayer(
  ctx: CanvasRenderingContext2D,
  layer: EditorLayer,
  layerImages?: Map<string, HTMLImageElement>,
  hideLayerId?: string | null,
) {
  if (!layer.visible) return
  if (layer.id === hideLayerId) return
  ctx.save()
  switch (layer.type) {
    case EditorLayerType.ARROW: drawArrow(ctx, layer); break
    case EditorLayerType.RECT: drawRect(ctx, layer); break
    case EditorLayerType.ELLIPSE: drawEllipse(ctx, layer); break
    case EditorLayerType.HIGHLIGHT: drawHighlight(ctx, layer); break
    case EditorLayerType.MOSAIC: break
    case EditorLayerType.TEXT: drawRichText(ctx, layer); break
    case EditorLayerType.IMAGE: drawEditorImage(ctx, layer, layerImages); break
    case EditorLayerType.FREEHAND: drawFreehand(ctx, layer); break
    case EditorLayerType.STICKER: drawSticker(ctx, layer); break
  }
  ctx.restore()
}

function drawArrow(ctx: CanvasRenderingContext2D, l: ArrowLayer) {
  const cx = (l.x1 + l.x2) / 2, cy = (l.y1 + l.y2) / 2
  ctx.save()
  if (l.rotation) {
    ctx.translate(cx, cy)
    ctx.rotate((l.rotation * Math.PI) / 180)
    ctx.translate(-cx, -cy)
  }

  const angle = Math.atan2(l.y2 - l.y1, l.x2 - l.x1)
  const headLen = Math.max(l.lineWidth * 4, 12)
  const headW = headLen * 0.5
  const baseX = l.x2 - headLen * Math.cos(angle)
  const baseY = l.y2 - headLen * Math.sin(angle)

  ctx.strokeStyle = l.color
  ctx.lineWidth = l.lineWidth
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'
  ctx.beginPath()
  ctx.moveTo(l.x1, l.y1)
  ctx.lineTo(baseX, baseY)
  ctx.stroke()

  ctx.fillStyle = l.color
  ctx.beginPath()
  ctx.moveTo(l.x2, l.y2)
  ctx.lineTo(
    baseX + headW * Math.sin(angle),
    baseY - headW * Math.cos(angle),
  )
  ctx.lineTo(
    baseX - headW * Math.sin(angle),
    baseY + headW * Math.cos(angle),
  )
  ctx.closePath()
  ctx.fill()
  ctx.restore()
}

function drawRect(ctx: CanvasRenderingContext2D, l: RectLayer) {
  ctx.strokeStyle = l.color
  ctx.lineWidth = l.lineWidth
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'
  ctx.strokeRect(l.x, l.y, l.w, l.h)
}

function drawEllipse(ctx: CanvasRenderingContext2D, l: EllipseLayer) {
  if (l.rx <= 0 || l.ry <= 0) return
  ctx.strokeStyle = l.color
  ctx.lineWidth = l.lineWidth
  ctx.beginPath()
  ctx.ellipse(l.cx, l.cy, l.rx, l.ry, 0, 0, Math.PI * 2)
  ctx.stroke()
}

function drawHighlight(ctx: CanvasRenderingContext2D, l: HighlightLayer) {
  ctx.globalAlpha = 0.35
  ctx.fillStyle = l.color
  ctx.fillRect(l.x, l.y, l.w, l.h)
}

function drawRichText(ctx: CanvasRenderingContext2D, l: EditorTextLayer) {
  if (!l.text) return
  ctx.save()
  if (l.rotation) {
    ctx.translate(l.x, l.y)
    ctx.rotate((l.rotation * Math.PI) / 180)
    ctx.translate(-l.x, -l.y)
  }

  ctx.font = `${l.fontStyle.toLowerCase()} ${cssFontWeight(l.fontWeight)} ${l.fontSize}px ${l.fontFamily}`
  ctx.textBaseline = 'middle'
  if (l.letterSpacing) (ctx as any).letterSpacing = `${l.letterSpacing}px`

  const lines = wrapText(ctx, l.text, l.maxWidth)
  const lineH = l.fontSize * 1.15
  const totalH = lines.length * lineH
  const startY = l.y - totalH / 2 + lineH / 2

  for (let i = 0; i < lines.length; i++) {
    const lineY = startY + i * lineH
    let lineX: number
    if (l.textAlign === EditorTextAlign.LEFT) lineX = l.x - l.maxWidth / 2
    else if (l.textAlign === EditorTextAlign.RIGHT) lineX = l.x + l.maxWidth / 2
    else lineX = l.x
    ctx.textAlign = l.textAlign.toLowerCase() as CanvasTextAlign

    ctx.shadowColor = 'transparent'
    ctx.shadowBlur = 0
    ctx.shadowOffsetX = 0
    ctx.shadowOffsetY = 0
    if (l.glow) {
      ctx.shadowColor = l.color
      ctx.shadowBlur = l.textShadow.blur || 20
    } else if (l.textShadow.blur > 0 || l.textShadow.offsetX || l.textShadow.offsetY) {
      ctx.shadowColor = l.textShadow.color
      ctx.shadowBlur = l.textShadow.blur
      ctx.shadowOffsetX = l.textShadow.offsetX
      ctx.shadowOffsetY = l.textShadow.offsetY
    }

    if (l.textStroke.width > 0) {
      ctx.strokeStyle = l.textStroke.color
      ctx.lineWidth = l.textStroke.width
      ctx.lineJoin = 'round'
      ctx.miterLimit = 2
      ctx.strokeText(lines[i]!, lineX, lineY)
    }

    ctx.fillStyle = l.color
    ctx.fillText(lines[i]!, lineX, lineY)
  }

  if (l.letterSpacing) (ctx as any).letterSpacing = '0px'
  ctx.restore()
}

function drawEditorImage(
  ctx: CanvasRenderingContext2D,
  l: EditorImageLayer,
  layerImages?: Map<string, HTMLImageElement>,
) {
  const img = layerImages?.get(l.id)
  if (!img) return
  ctx.save()
  ctx.globalAlpha = l.opacity
  if (l.rotation) {
    ctx.translate(l.x, l.y)
    ctx.rotate((l.rotation * Math.PI) / 180)
    ctx.translate(-l.x, -l.y)
  }
  ctx.drawImage(img, l.x - l.w / 2, l.y - l.h / 2, l.w, l.h)
  ctx.restore()
}

function drawFreehand(ctx: CanvasRenderingContext2D, l: FreehandLayer) {
  if (l.points.length < 2) return
  ctx.strokeStyle = l.color
  ctx.lineWidth = l.lineWidth
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'
  ctx.beginPath()
  ctx.moveTo(l.points[0]!.x, l.points[0]!.y)
  for (let i = 1; i < l.points.length - 1; i++) {
    const curr = l.points[i]!
    const next = l.points[i + 1]!
    const midX = (curr.x + next.x) / 2
    const midY = (curr.y + next.y) / 2
    ctx.quadraticCurveTo(curr.x, curr.y, midX, midY)
  }
  const last = l.points[l.points.length - 1]!
  ctx.lineTo(last.x, last.y)
  ctx.stroke()
}

export const STICKER_PALETTE: Record<string, { bg: string; text: string }> = {
  '#fef08a': { bg: '#fef08a', text: '#713f12' },
  '#bbf7d0': { bg: '#bbf7d0', text: '#14532d' },
  '#bfdbfe': { bg: '#bfdbfe', text: '#1e3a5f' },
  '#fbcfe8': { bg: '#fbcfe8', text: '#831843' },
  '#fde68a': { bg: '#fde68a', text: '#78350f' },
}

const STICKER_PADDING = 14

export function measureStickerSize(l: StickerLayer): { w: number; h: number } {
  const canvas = document.createElement('canvas')
  const ctx = canvas.getContext('2d')!
  ctx.font = `${l.fontStyle.toLowerCase()} ${cssFontWeight(l.fontWeight)} ${l.fontSize}px Inter, sans-serif`
  const minW = l.fontSize * 4
  const maxW = Math.max(l.w, minW)
  const textMaxW = maxW - STICKER_PADDING * 2
  const lines = wrapText(ctx, l.text, textMaxW)
  const lineH = l.fontSize * 1.3
  const h = Math.max(l.fontSize * 2, lines.length * lineH + STICKER_PADDING * 2)
  return { w: maxW, h }
}

function drawSticker(ctx: CanvasRenderingContext2D, l: StickerLayer) {
  ctx.save()
  if (l.rotation) {
    ctx.translate(l.x, l.y)
    ctx.rotate((l.rotation * Math.PI) / 180)
    ctx.translate(-l.x, -l.y)
  }

  const x = l.x - l.w / 2, y = l.y - l.h / 2
  const palette = STICKER_PALETTE[l.color] ?? { bg: l.color, text: '#374151' }
  const r = 8

  ctx.fillStyle = palette.bg
  ctx.beginPath()
  ctx.roundRect(x, y, l.w, l.h, r)
  ctx.fill()

  if (l.text) {
    ctx.fillStyle = palette.text
    ctx.font = `${l.fontStyle.toLowerCase()} ${cssFontWeight(l.fontWeight)} ${l.fontSize}px Inter, sans-serif`
    ctx.textAlign = 'left'
    ctx.textBaseline = 'top'
    const maxW = l.w - STICKER_PADDING * 2
    const lines = wrapText(ctx, l.text, maxW)
    const lineH = l.fontSize * 1.3
    const totalH = lines.length * lineH
    const startY = y + (l.h - totalH) / 2
    for (let i = 0; i < lines.length; i++) {
      const lineY = startY + i * lineH
      if (lineY + lineH > y + l.h + 2) break
      ctx.fillText(lines[i]!, x + STICKER_PADDING, lineY)
    }
  }

  ctx.restore()
}
