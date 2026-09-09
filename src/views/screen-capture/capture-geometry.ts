import type { FrameBounds, FramePoint, SelectionRect } from './selection-model'

export interface CssViewportRect extends FrameBounds {
  left: number
  top: number
}

function requireFinite(values: number[], label: string): void {
  if (values.some((value) => !Number.isFinite(value))) throw new RangeError(`invalid capture ${label}`)
}

function requireBounds(bounds: FrameBounds, label: string): void {
  requireFinite([bounds.width, bounds.height], label)
  if (bounds.width <= 0 || bounds.height <= 0) throw new RangeError(`invalid capture ${label}`)
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

export function cssPointToFrame(point: FramePoint, viewport: CssViewportRect, frame: FrameBounds): FramePoint {
  requireFinite([point.x, point.y, viewport.left, viewport.top], 'coordinates')
  requireBounds(viewport, 'viewport')
  requireBounds(frame, 'frame')
  return {
    x: clamp(((point.x - viewport.left) * frame.width) / viewport.width, 0, frame.width),
    y: clamp(((point.y - viewport.top) * frame.height) / viewport.height, 0, frame.height),
  }
}

export function frameRectToCss(rect: SelectionRect, viewport: CssViewportRect, frame: FrameBounds): SelectionRect {
  requireFinite([rect.x, rect.y, rect.width, rect.height, viewport.left, viewport.top], 'coordinates')
  requireBounds(viewport, 'viewport')
  requireBounds(frame, 'frame')
  return {
    x: viewport.left + (rect.x * viewport.width) / frame.width,
    y: viewport.top + (rect.y * viewport.height) / frame.height,
    width: (rect.width * viewport.width) / frame.width,
    height: (rect.height * viewport.height) / frame.height,
  }
}

export function selectionToExportRect(selection: SelectionRect, frame: FrameBounds): SelectionRect {
  requireFinite([selection.x, selection.y, selection.width, selection.height], 'selection')
  requireBounds(frame, 'frame')
  if (selection.width <= 0 || selection.height <= 0) throw new RangeError('invalid capture selection')

  const left = clamp(Math.floor(selection.x), 0, frame.width)
  const top = clamp(Math.floor(selection.y), 0, frame.height)
  const right = clamp(Math.ceil(selection.x + selection.width), 0, frame.width)
  const bottom = clamp(Math.ceil(selection.y + selection.height), 0, frame.height)
  if (right <= left || bottom <= top) throw new RangeError('invalid capture selection')
  return { x: left, y: top, width: right - left, height: bottom - top }
}
