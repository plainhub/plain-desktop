/**
 * Map a client point to normalized 0..1 video coordinates.
 *
 * The canvas element keeps the video's intrinsic aspect (max-width/max-height
 * fit), so its CSS rect is exactly the rendered video rect — normalizing
 * against it cannot drift from the picture. Out-of-rect points are clamped
 * instead of dropped: touching just outside the picture behaves like touching
 * the screen edge on a real phone (which is what makes bottom-edge swipe-up
 * home gestures reachable with a mouse).
 */
export function normalizeCoords(
  clientX: number,
  clientY: number,
  canvasRect: DOMRect,
  canvasEl: HTMLCanvasElement,
): { x: number; y: number } | null {
  // canvas.width of 0 = no video content yet, nothing to map against
  if (!canvasEl.width || !canvasEl.height || !canvasRect.width || !canvasRect.height) return null
  const x = (clientX - canvasRect.left) / canvasRect.width
  const y = (clientY - canvasRect.top) / canvasRect.height
  return {
    x: Math.max(0, Math.min(1, x)),
    y: Math.max(0, Math.min(1, y)),
  }
}
