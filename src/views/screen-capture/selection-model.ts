export interface FramePoint {
  x: number
  y: number
}

export interface FrameBounds {
  width: number
  height: number
}

export interface SelectionRect extends FramePoint {
  width: number
  height: number
}

export type SelectionHandle = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w'
export const MIN_SELECTION_SIZE = 20

type Interaction =
  | { kind: 'create'; pointerId: number; anchor: FramePoint; previous: SelectionRect | null }
  | { kind: 'move'; pointerId: number; pointer: FramePoint; original: SelectionRect }
  | { kind: 'resize'; pointerId: number; handle: SelectionHandle; original: SelectionRect }

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

function clampPoint(point: FramePoint, bounds: FrameBounds): FramePoint {
  if (!Number.isFinite(point.x) || !Number.isFinite(point.y)) throw new RangeError('invalid capture point')
  return {
    x: clamp(point.x, 0, bounds.width),
    y: clamp(point.y, 0, bounds.height),
  }
}

function normalizedRect(first: FramePoint, second: FramePoint): SelectionRect {
  const left = Math.floor(Math.min(first.x, second.x))
  const top = Math.floor(Math.min(first.y, second.y))
  const right = Math.ceil(Math.max(first.x, second.x))
  const bottom = Math.ceil(Math.max(first.y, second.y))
  return {
    x: left,
    y: top,
    width: right - left,
    height: bottom - top,
  }
}

function contains(rect: SelectionRect, point: FramePoint): boolean {
  return point.x >= rect.x && point.x <= rect.x + rect.width && point.y >= rect.y && point.y <= rect.y + rect.height
}

function cloneRect(rect: SelectionRect | null): SelectionRect | null {
  return rect ? { ...rect } : null
}

export function handleCenters(rect: SelectionRect): Record<SelectionHandle, FramePoint> {
  const middleX = rect.x + rect.width / 2
  const middleY = rect.y + rect.height / 2
  const right = rect.x + rect.width
  const bottom = rect.y + rect.height
  return {
    nw: { x: rect.x, y: rect.y },
    n: { x: middleX, y: rect.y },
    ne: { x: right, y: rect.y },
    e: { x: right, y: middleY },
    se: { x: right, y: bottom },
    s: { x: middleX, y: bottom },
    sw: { x: rect.x, y: bottom },
    w: { x: rect.x, y: middleY },
  }
}

export function findSelectionHandle(rect: SelectionRect, point: FramePoint, hitRadius: number): SelectionHandle | null {
  const centers = handleCenters(rect)
  const hitRadiusSquared = hitRadius * hitRadius
  for (const handle of Object.keys(centers) as SelectionHandle[]) {
    const center = centers[handle]
    const dx = point.x - center.x
    const dy = point.y - center.y
    if (dx * dx + dy * dy <= hitRadiusSquared) return handle
  }
  return null
}

export function resizeSelection(rect: SelectionRect, handle: SelectionHandle, pointer: FramePoint, bounds: FrameBounds): SelectionRect {
  const point = clampPoint(pointer, bounds)
  const left = rect.x
  const top = rect.y
  const right = rect.x + rect.width
  const bottom = rect.y + rect.height

  const horizontal = handle.includes('w')
    ? controlledInterval(point.x, right, 'before', bounds.width)
    : handle.includes('e')
      ? controlledInterval(point.x, left, 'after', bounds.width)
      : { start: left, end: right }
  const vertical = handle.includes('n')
    ? controlledInterval(point.y, bottom, 'before', bounds.height)
    : handle.includes('s')
      ? controlledInterval(point.y, top, 'after', bounds.height)
      : { start: top, end: bottom }
  return {
    x: horizontal.start,
    y: vertical.start,
    width: horizontal.end - horizontal.start,
    height: vertical.end - vertical.start,
  }
}

function controlledInterval(moving: number, anchor: number, preferredSide: 'before' | 'after', boundary: number): { start: number; end: number } {
  let side = moving === anchor ? preferredSide : moving < anchor ? 'before' : 'after'
  if (side === 'before' && anchor < MIN_SELECTION_SIZE) side = 'after'
  if (side === 'after' && boundary - anchor < MIN_SELECTION_SIZE) side = 'before'
  if (side === 'before') {
    const end = anchor
    const start = clamp(Math.min(Math.floor(moving), end - MIN_SELECTION_SIZE), 0, boundary)
    return { start, end }
  }
  const start = anchor
  const end = clamp(Math.max(Math.ceil(moving), start + MIN_SELECTION_SIZE), 0, boundary)
  return { start, end }
}

export function captureToolbarWorkArea(viewport: FrameBounds, viewportScreenOrigin: FramePoint, availableScreenArea: SelectionRect): SelectionRect {
  const values = [viewport.width, viewport.height, viewportScreenOrigin.x, viewportScreenOrigin.y, availableScreenArea.x, availableScreenArea.y, availableScreenArea.width, availableScreenArea.height]
  const fallback = { x: 0, y: 0, width: viewport.width, height: viewport.height }
  if (values.some((value) => !Number.isFinite(value)) || viewport.width <= 0 || viewport.height <= 0 || availableScreenArea.width <= 0 || availableScreenArea.height <= 0) {
    return fallback
  }

  const left = Math.max(viewportScreenOrigin.x, availableScreenArea.x)
  const top = Math.max(viewportScreenOrigin.y, availableScreenArea.y)
  const right = Math.min(viewportScreenOrigin.x + viewport.width, availableScreenArea.x + availableScreenArea.width)
  const bottom = Math.min(viewportScreenOrigin.y + viewport.height, availableScreenArea.y + availableScreenArea.height)
  if (right <= left || bottom <= top) return fallback
  return {
    x: left - viewportScreenOrigin.x,
    y: top - viewportScreenOrigin.y,
    width: right - left,
    height: bottom - top,
  }
}

export function placeCaptureToolbar(
  selection: SelectionRect,
  toolbar: FrameBounds,
  viewport: FrameBounds,
  gap = 8,
  visibleArea: SelectionRect = { x: 0, y: 0, width: viewport.width, height: viewport.height }
): FramePoint {
  const area = captureToolbarWorkArea(viewport, { x: 0, y: 0 }, visibleArea)
  const areaRight = area.x + area.width
  const areaBottom = area.y + area.height
  const maxX = Math.max(area.x, areaRight - toolbar.width)
  const maxY = Math.max(area.y, areaBottom - toolbar.height)
  const centeredX = clamp(selection.x + selection.width / 2 - toolbar.width / 2, area.x, maxX)
  const centeredY = clamp(selection.y + selection.height / 2 - toolbar.height / 2, area.y, maxY)
  const below = selection.y + selection.height + gap
  const above = selection.y - gap - toolbar.height
  const right = selection.x + selection.width + gap
  const left = selection.x - gap - toolbar.width

  if (below >= area.y && below + toolbar.height <= areaBottom) return { x: centeredX, y: below }
  if (above >= area.y && above + toolbar.height <= areaBottom) return { x: centeredX, y: above }
  if (right >= area.x && right + toolbar.width <= areaRight) return { x: right, y: centeredY }
  if (left >= area.x && left + toolbar.width <= areaRight) return { x: left, y: centeredY }

  return { x: centeredX, y: clamp(below, area.y, maxY) }
}

export class CaptureSelection {
  rect: SelectionRect | null
  private interaction: Interaction | null = null

  constructor(
    private readonly bounds: FrameBounds,
    initial: SelectionRect | null = null
  ) {
    if (!Number.isSafeInteger(bounds.width) || !Number.isSafeInteger(bounds.height) || bounds.width <= 0 || bounds.height <= 0) {
      throw new RangeError('invalid capture frame bounds')
    }
    if (
      initial &&
      (!Object.values(initial).every(Number.isSafeInteger) ||
        initial.x < 0 ||
        initial.y < 0 ||
        initial.width < MIN_SELECTION_SIZE ||
        initial.height < MIN_SELECTION_SIZE ||
        initial.x + initial.width > bounds.width ||
        initial.y + initial.height > bounds.height)
    ) {
      throw new RangeError('invalid capture selection')
    }
    this.rect = cloneRect(initial)
  }

  begin(pointerId: number, pointer: FramePoint, handle?: SelectionHandle): void {
    if (!Number.isInteger(pointerId)) throw new RangeError('invalid capture pointer id')
    const point = clampPoint(pointer, this.bounds)
    if (this.rect && handle) {
      this.interaction = { kind: 'resize', pointerId, handle, original: { ...this.rect } }
      return
    }
    if (this.rect && contains(this.rect, point)) {
      this.interaction = { kind: 'move', pointerId, pointer: point, original: { ...this.rect } }
      return
    }
    this.interaction = { kind: 'create', pointerId, anchor: point, previous: cloneRect(this.rect) }
    this.rect = { x: point.x, y: point.y, width: 0, height: 0 }
  }

  update(pointerId: number, pointer: FramePoint): void {
    if (!this.interaction || this.interaction.pointerId !== pointerId) return
    const point = clampPoint(pointer, this.bounds)
    if (this.interaction.kind === 'create') {
      this.rect = normalizedRect(this.interaction.anchor, point)
      return
    }
    if (this.interaction.kind === 'resize') {
      this.rect = resizeSelection(this.interaction.original, this.interaction.handle, point, this.bounds)
      return
    }

    const { original } = this.interaction
    const x = clamp(Math.round(original.x + point.x - this.interaction.pointer.x), 0, this.bounds.width - original.width)
    const y = clamp(Math.round(original.y + point.y - this.interaction.pointer.y), 0, this.bounds.height - original.height)
    this.rect = { x, y, width: original.width, height: original.height }
  }

  end(pointerId: number): void {
    if (!this.interaction || this.interaction.pointerId !== pointerId) return
    if (this.rect && (this.rect.width < MIN_SELECTION_SIZE || this.rect.height < MIN_SELECTION_SIZE)) this.rect = null
    this.interaction = null
  }

  cancelInteraction(pointerId?: number): void {
    if (!this.interaction || (pointerId !== undefined && this.interaction.pointerId !== pointerId)) return
    if (this.interaction.kind === 'create') this.rect = cloneRect(this.interaction.previous)
    else this.rect = { ...this.interaction.original }
    this.interaction = null
  }
}
