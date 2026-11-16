import { expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { drawSelectionRect } from '@/views/image-editor/utils/renderer'
import { useEditorTransform } from '@/views/image-editor/composables/useEditorTransform'
import { useImageEditorDoc } from '@/views/image-editor/composables/useImageEditorDoc'
import type { ArrowLayer } from '@/views/image-editor/utils/types'

const arrow: ArrowLayer = { id: 'arrow', type: 'arrow', visible: true, name: 'Arrow', x1: 100, y1: 100, x2: 300, y2: 150, color: '#ef4444', lineWidth: 4, rotation: 0 }

it.each([.5, 1, 3])('keeps visible arrow handles and hit regions fixed at display scale %s', scale => {
  let currentScale = scale
  const stack: number[] = []
  const radii: number[] = []
  const ctx = {
    save: () => stack.push(currentScale), restore: () => { currentScale = stack.pop()! },
    scale: (x: number) => { currentScale *= x },
    lineWidth: 0,
    arc: (_x: number, _y: number, radius: number) => radii.push((radius + ctx.lineWidth / 2) * currentScale),
    setLineDash: vi.fn(), strokeRect: vi.fn(), beginPath: vi.fn(), fill: vi.fn(), stroke: vi.fn(), moveTo: vi.fn(), lineTo: vi.fn(),
  }
  drawSelectionRect(ctx as unknown as CanvasRenderingContext2D, arrow, scale)
  expect(radii.slice(0, 4)).toEqual([6, 6, 6, 6])
  expect(radii[4]).toBe(12)
  const doc = useImageEditorDoc()
  doc.addLayer(arrow)
  const transform = useEditorTransform([arrow], ref(arrow.id), doc, vi.fn(), () => scale)
  const corner = { x: 100 - 8 / scale, y: 100 - 8 / scale }
  expect(transform.hitTestHandle(corner.x, corner.y)).toBe('scale-nw')
  expect(transform.hitTestHandle(corner.x - 13 / scale, corner.y)).toBeNull()
  expect(transform.hitTestHandle(200, 100 - 44 / scale)).toBe('rotate')
  expect(transform.hitTestHandle(200 + 17 / scale, 100 - 44 / scale)).toBeNull()
  doc.ydoc.destroy()
})
