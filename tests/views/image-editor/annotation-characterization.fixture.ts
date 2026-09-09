import type { ArrowLayer, CanvasSize, EditorLayer, EllipseLayer, FreehandLayer, MosaicLayer, RectLayer } from '@/views/image-editor/utils/types'
import { renderEditorCanvas } from '@/views/image-editor/utils/renderer'

export const CHARACTERIZATION_CANVAS_SIZE: CanvasSize = {
  width: 64,
  height: 48,
}

const mosaic: MosaicLayer = {
  id: 'mosaic-fixture',
  type: 'mosaic',
  visible: true,
  name: 'Mosaic 1',
  x: 4,
  y: 4,
  w: 20,
  h: 16,
  blockSize: 4,
}

const rect: RectLayer = {
  id: 'rect-fixture',
  type: 'rect',
  visible: true,
  name: 'Rect 1',
  x: 9,
  y: 7,
  w: 28,
  h: 18,
  color: '#ef4444',
  lineWidth: 2,
}

const ellipse: EllipseLayer = {
  id: 'ellipse-fixture',
  type: 'ellipse',
  visible: true,
  name: 'Ellipse 1',
  cx: 46,
  cy: 15,
  rx: 10,
  ry: 7,
  color: '#22c55e',
  lineWidth: 4,
}

const arrow: ArrowLayer = {
  id: 'arrow-fixture',
  type: 'arrow',
  visible: true,
  name: 'Arrow 1',
  x1: 8,
  y1: 39,
  x2: 32,
  y2: 27,
  color: '#3b82f6',
  lineWidth: 3,
  rotation: 0,
}

const freehand: FreehandLayer = {
  id: 'freehand-fixture',
  type: 'freehand',
  visible: true,
  name: 'Brush 1',
  points: [
    { x: 37, y: 38 },
    { x: 42, y: 32 },
    { x: 48, y: 39 },
    { x: 55, y: 30 },
    { x: 60, y: 37 },
  ],
  color: '#8b5cf6',
  lineWidth: 3,
}

/**
 * Fixed, non-text layers used as the independent before/after render oracle.
 * Mosaic comes first so its pixels depend only on the deterministic source;
 * later vector layers make ordering regressions observable in the final hash.
 */
export const CHARACTERIZATION_LAYERS: readonly EditorLayer[] = [mosaic, rect, ellipse, arrow, freehand]

export async function createDeterministicSourceImage(): Promise<HTMLImageElement> {
  const { width, height } = CHARACTERIZATION_CANVAS_SIZE
  const source = document.createElement('canvas')
  source.width = width
  source.height = height
  const ctx = source.getContext('2d')!
  const pixels = ctx.createImageData(width, height)

  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4
      pixels.data[i] = (x * 17 + y * 3) % 256
      pixels.data[i + 1] = (x * 5 + y * 19) % 256
      pixels.data[i + 2] = (x * 11 + y * 7) % 256
      pixels.data[i + 3] = 255
    }
  }
  ctx.putImageData(pixels, 0, 0)

  const image = new Image()
  const loaded = new Promise<void>((resolve, reject) => {
    image.onload = () => resolve()
    image.onerror = () => reject(new Error('Failed to decode characterization fixture'))
  })
  image.src = source.toDataURL('image/png')
  await loaded
  return image
}

export function renderCharacterizationPixels(source: HTMLImageElement): ImageData {
  const { width, height } = CHARACTERIZATION_CANVAS_SIZE
  const output = document.createElement('canvas')
  output.width = width
  output.height = height
  const ctx = output.getContext('2d', { willReadFrequently: true })!

  renderEditorCanvas(
    ctx,
    source,
    { x: 0, y: 0 },
    CHARACTERIZATION_LAYERS.map((layer) => structuredClone(layer)),
    CHARACTERIZATION_CANVAS_SIZE,
    'transparent',
    null,
    new Map(),
    null,
    100
  )

  return ctx.getImageData(0, 0, width, height)
}

export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('')
}
