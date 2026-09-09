import type {
  EditorLayer, MosaicLayer, CanvasSize, EditorRasterSource,
} from '@/views/image-editor/utils/types'
import { getEditorLayerBounds, getEditorRasterSize } from '@/views/image-editor/utils/types'
import { drawSelectionBorder, drawHoverBorder } from '@/views/image-editor/utils/selection-border'
import { drawLayer } from '@/views/image-editor/utils/editor-draw-layers'

export function applyMosaic(ctx: CanvasRenderingContext2D, l: MosaicLayer) {
  if (!l.visible || l.w < 2 || l.h < 2) return
  const t = ctx.getTransform()
  const s = t.a
  ctx.setTransform(1, 0, 0, 1, 0, 0)
  const bs = Math.max(1, Math.round(l.blockSize * s))
  const ix = Math.max(0, Math.floor(l.x * s))
  const iy = Math.max(0, Math.floor(l.y * s))
  const iw = Math.min(Math.ceil(l.w * s), ctx.canvas.width - ix)
  const ih = Math.min(Math.ceil(l.h * s), ctx.canvas.height - iy)
  if (iw <= 0 || ih <= 0) { ctx.setTransform(t); return }

  const imageData = ctx.getImageData(ix, iy, iw, ih)
  const data = imageData.data
  for (let by = 0; by < ih; by += bs) {
    for (let bx = 0; bx < iw; bx += bs) {
      const bw = Math.min(bs, iw - bx)
      const bh = Math.min(bs, ih - by)
      let r = 0, g = 0, b = 0, count = 0
      for (let py = by; py < by + bh; py++) {
        for (let px = bx; px < bw + bx; px++) {
          const idx = (py * iw + px) * 4
          r += data[idx]!; g += data[idx + 1]!; b += data[idx + 2]!; count++
        }
      }
      r = Math.round(r / count); g = Math.round(g / count); b = Math.round(b / count)
      for (let py = by; py < by + bh; py++) {
        for (let px = bx; px < bw + bx; px++) {
          const idx = (py * iw + px) * 4
          data[idx] = r; data[idx + 1] = g; data[idx + 2] = b
        }
      }
    }
  }
  ctx.putImageData(imageData, ix, iy)
  ctx.setTransform(t)
}

export function drawCheckerboard(ctx: CanvasRenderingContext2D, cw: number, ch: number) {
  const size = 16
  ctx.fillStyle = '#f8f8f8'
  ctx.fillRect(0, 0, cw, ch)
  ctx.fillStyle = '#e5e5e5'
  for (let y = 0; y < ch; y += size) {
    for (let x = 0; x < cw; x += size) {
      if ((Math.floor(x / size) + Math.floor(y / size)) % 2 === 0) {
        ctx.fillRect(x, y, size, size)
      }
    }
  }
}

export function renderEditorCanvas(
  ctx: CanvasRenderingContext2D,
  img: EditorRasterSource | null,
  imgOffset: { x: number; y: number },
  layers: EditorLayer[],
  size: CanvasSize,
  bgColor: string,
  previewLayer?: EditorLayer | null,
  layerImages?: Map<string, HTMLImageElement>,
  hideLayerId?: string | null,
  imgAlpha?: number,
  showTransparencyGrid = true,
) {
  const { width: cw, height: ch } = size
  if (showTransparencyGrid) drawCheckerboard(ctx, cw, ch)
  if (bgColor !== 'transparent') {
    if (bgColor.startsWith('gradient:') || bgColor.startsWith('linear-gradient') || bgColor.startsWith('radial-gradient')) {
      ctx.fillStyle = parseGradient(ctx, bgColor, cw, ch)
    } else {
      ctx.fillStyle = bgColor
    }
    ctx.fillRect(0, 0, cw, ch)
  }
  if (img) {
    const sourceSize = getEditorRasterSize(img)
    const alpha = imgAlpha != null ? imgAlpha / 100 : 1
    if (alpha < 1) ctx.globalAlpha = alpha
    ctx.drawImage(img, imgOffset.x, imgOffset.y, sourceSize.width, sourceSize.height)
    if (alpha < 1) ctx.globalAlpha = 1
  }
  const allLayers = previewLayer ? [...layers, previewLayer] : layers

  for (const l of allLayers) {
    if (l.type === 'mosaic') {
      if (l.visible) applyMosaic(ctx, l as MosaicLayer)
    } else {
      drawLayer(ctx, l, layerImages, hideLayerId)
    }
  }
}

function parseGradient(
  ctx: CanvasRenderingContext2D,
  value: string,
  w: number,
  h: number,
): CanvasGradient {
  let angle = 0
  let colors: string[] = ['#000000', '#ffffff']

  if (value.startsWith('linear-gradient')) {
    const inner = value.replace(/^linear-gradient\(/, '').replace(/\)$/, '')
    const parts = inner.split(',')
    const first = parts[0]!.trim()
    if (first.endsWith('deg')) {
      angle = parseFloat(first)
      colors = parts.slice(1).map(s => s.trim()).filter(Boolean)
    } else {
      colors = parts.map(s => s.trim()).filter(Boolean)
    }
  } else if (value.startsWith('gradient:')) {
    const parts = value.split(':')
    angle = parseFloat(parts[2] ?? '0')
    colors = [parts[3] ?? '#000000', parts[4] ?? '#ffffff']
  }

  const rad = ((angle - 90) * Math.PI) / 180
  const cx = w / 2
  const cy = h / 2
  const diag = Math.sqrt(w * w + h * h) / 2
  const dx = Math.cos(rad) * diag
  const dy = Math.sin(rad) * diag

  const grad = ctx.createLinearGradient(cx - dx, cy - dy, cx + dx, cy + dy)
  for (let i = 0; i < colors.length; i++) {
    grad.addColorStop(i / Math.max(1, colors.length - 1), colors[i]!)
  }
  return grad
}

const SEL_PAD = 10

export interface SelectionLayout {
  bounds: { x: number; y: number; w: number; h: number }
  pad: number
  cx: number
  cy: number
  rotation: number
}

export function getSelectionLayout(layer: EditorLayer): SelectionLayout {
  const bounds = getEditorLayerBounds(layer)
  const rot = ('rotation' in layer) ? (layer as any).rotation ?? 0 : 0
  let cx: number, cy: number
  if (layer.type === 'text' || layer.type === 'image' || layer.type === 'sticker') {
    cx = (layer as any).x; cy = (layer as any).y
  } else if (layer.type === 'arrow') {
    cx = (layer.x1 + layer.x2) / 2; cy = (layer.y1 + layer.y2) / 2
  } else if (layer.type === 'ellipse') {
    cx = layer.cx; cy = layer.cy
  } else {
    cx = bounds.x + bounds.w / 2; cy = bounds.y + bounds.h / 2
  }
  return { bounds, pad: SEL_PAD, cx, cy, rotation: rot }
}

export function drawSelectionRect(
  ctx: CanvasRenderingContext2D,
  layer: EditorLayer,
  _scale: number,
) {
  const { bounds, pad, cx, cy, rotation } = getSelectionLayout(layer)
  const hasHandles = layer.type === 'text' || layer.type === 'image' || layer.type === 'arrow' || layer.type === 'sticker'
  drawSelectionBorder(ctx, {
    x: bounds.x, y: bounds.y, w: bounds.w, h: bounds.h,
    cx, cy, rotation,
  }, {
    color: '#3b82f6',
    pad,
    showEdgeHandles: layer.type === 'text' || layer.type === 'image',
    showRotationHandle: hasHandles,
  })
}

export function drawHoverRect(
  ctx: CanvasRenderingContext2D,
  layer: EditorLayer,
) {
  const { bounds, pad, cx, cy, rotation } = getSelectionLayout(layer)
  drawHoverBorder(ctx, {
    x: bounds.x, y: bounds.y, w: bounds.w, h: bounds.h,
    cx, cy, rotation,
  }, pad)
}
