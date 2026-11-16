/**
 * Draw a selection border with transform handles on a canvas context.
 * Shared by ImageEditor (renderer.ts) and MakerEditor (useThumbnailOverlay.ts).
 */

export interface SelectionBounds {
  x: number
  y: number
  w: number
  h: number
  cx: number
  cy: number
  rotation: number
}

export interface DrawSelectionOptions {
  color?: string
  pad?: number
  showEdgeHandles?: boolean
  showRotationHandle?: boolean
}

export const SELECTION = { padding: 8, cornerRadius: 6, rotationRadius: 12, rotationOffset: 36, cornerHitRadius: 12, rotationHitRadius: 16, edgeHitRadius: 16 } as const

export function drawSelectionBorder(
  ctx: CanvasRenderingContext2D,
  bounds: SelectionBounds,
  opts: DrawSelectionOptions = {},
) {
  const color = opts.color ?? '#3b82f6'
  const pad = opts.pad ?? SELECTION.padding
  const showEdge = opts.showEdgeHandles ?? true
  const showRotation = opts.showRotationHandle ?? true

  ctx.save()

  if (bounds.rotation) {
    ctx.translate(bounds.cx, bounds.cy)
    ctx.rotate((bounds.rotation * Math.PI) / 180)
    ctx.translate(-bounds.cx, -bounds.cy)
  }

  ctx.strokeStyle = color
  ctx.lineWidth = 2
  ctx.setLineDash([6, 4])
  ctx.strokeRect(bounds.x - pad, bounds.y - pad, bounds.w + pad * 2, bounds.h + pad * 2)
  ctx.setLineDash([])

  const corners = [
    [bounds.x - pad, bounds.y - pad],
    [bounds.x + bounds.w + pad, bounds.y - pad],
    [bounds.x - pad, bounds.y + bounds.h + pad],
    [bounds.x + bounds.w + pad, bounds.y + bounds.h + pad],
  ]
  for (const [hx, hy] of corners) {
    ctx.fillStyle = '#ffffff'
    ctx.strokeStyle = color
    ctx.lineWidth = 2
    ctx.beginPath()
    ctx.arc(hx!, hy!, SELECTION.cornerRadius - ctx.lineWidth / 2, 0, Math.PI * 2)
    ctx.fill()
    ctx.stroke()
  }

  if (showEdge) {
    const edgeMidY = bounds.y + bounds.h / 2
    const edgeHandles = [
      [bounds.x - pad, edgeMidY],
      [bounds.x + bounds.w + pad, edgeMidY],
    ]
    for (const [ex, ey] of edgeHandles) {
      ctx.fillStyle = '#ffffff'
      ctx.strokeStyle = color
      ctx.lineWidth = 2
      const hw = 5, hh = 14
      ctx.beginPath()
      ctx.roundRect(ex! - hw, ey! - hh, hw * 2, hh * 2, hw)
      ctx.fill()
      ctx.stroke()
    }
  }

  if (showRotation) {
    const handleX = bounds.x + bounds.w / 2
    const handleY = bounds.y - pad - SELECTION.rotationOffset
    ctx.strokeStyle = color
    ctx.lineWidth = 1.5
    ctx.beginPath()
    ctx.moveTo(handleX, bounds.y - pad)
    ctx.lineTo(handleX, handleY + SELECTION.rotationRadius)
    ctx.stroke()
    ctx.fillStyle = '#ffffff'
    ctx.strokeStyle = color
    ctx.lineWidth = 2.5
    ctx.beginPath()
    ctx.arc(handleX, handleY, SELECTION.rotationRadius - ctx.lineWidth / 2, 0, Math.PI * 2)
    ctx.fill()
    ctx.stroke()
    const hR = 7
    ctx.strokeStyle = color
    ctx.lineWidth = 2
    ctx.lineCap = 'round'
    ctx.beginPath()
    ctx.arc(handleX, handleY, hR, -Math.PI * 0.75, Math.PI * 0.15)
    ctx.stroke()
    const a1 = Math.PI * 0.15
    const a1x = handleX + hR * Math.cos(a1)
    const a1y = handleY + hR * Math.sin(a1)
    ctx.beginPath()
    ctx.moveTo(a1x - 1, a1y - 5)
    ctx.lineTo(a1x, a1y)
    ctx.lineTo(a1x + 5, a1y - 1)
    ctx.stroke()
    ctx.beginPath()
    ctx.arc(handleX, handleY, hR, Math.PI * 0.25, Math.PI * 1.15)
    ctx.stroke()
    const a2 = Math.PI * 1.15
    const a2x = handleX + hR * Math.cos(a2)
    const a2y = handleY + hR * Math.sin(a2)
    ctx.beginPath()
    ctx.moveTo(a2x + 1, a2y + 5)
    ctx.lineTo(a2x, a2y)
    ctx.lineTo(a2x - 5, a2y + 1)
    ctx.stroke()
    ctx.lineCap = 'butt'
  }

  ctx.restore()
}

/** Lightweight dashed border shown on hover (no handles). */
export function drawHoverBorder(
  ctx: CanvasRenderingContext2D,
  bounds: SelectionBounds,
  pad: number = SELECTION.padding,
) {
  ctx.save()
  if (bounds.rotation) {
    ctx.translate(bounds.cx, bounds.cy)
    ctx.rotate((bounds.rotation * Math.PI) / 180)
    ctx.translate(-bounds.cx, -bounds.cy)
  }
  ctx.strokeStyle = '#3b82f6'
  ctx.lineWidth = 1.5
  ctx.setLineDash([4, 4])
  ctx.strokeRect(bounds.x - pad, bounds.y - pad, bounds.w + pad * 2, bounds.h + pad * 2)
  ctx.setLineDash([])
  ctx.restore()
}
