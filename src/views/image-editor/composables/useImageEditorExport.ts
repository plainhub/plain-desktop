import type { Ref } from 'vue'
import type { CanvasSize, EditorLayer, EditorRasterSource } from '@/views/image-editor/utils/types'
import { renderEditorCanvas } from '@/views/image-editor/utils/renderer'

export interface EditorExportRect {
  x: number
  y: number
  width: number
  height: number
}

export type EditorExportError = 'invalid-selection' | 'context-unavailable' | 'encode-failed'

export type EditorExportResult<T> = { ok: true; value: T } | { ok: false; error: EditorExportError }

/** Export/download/copy functions for the image editor and ephemeral sessions. */
export function useImageEditorExport(
  canvasSize: Ref<CanvasSize>,
  sourceImg: Ref<EditorRasterSource | null>,
  imgOffset: { x: number; y: number },
  layers: EditorLayer[],
  bgColor: Ref<string>,
  layerImages: Map<string, HTMLImageElement>,
  imgAlpha: Ref<number>
) {
  function validSelection(selection: EditorExportRect): boolean {
    const { width, height } = canvasSize.value
    return (
      [selection.x, selection.y, selection.width, selection.height].every(Number.isInteger) &&
      selection.width > 0 &&
      selection.height > 0 &&
      selection.x >= 0 &&
      selection.y >= 0 &&
      selection.x + selection.width <= width &&
      selection.y + selection.height <= height
    )
  }

  function renderToTempCanvas(selection?: EditorExportRect, showTransparencyGrid = true): EditorExportResult<HTMLCanvasElement> {
    if (selection && !validSelection(selection)) return { ok: false, error: 'invalid-selection' }

    const rect = selection ?? {
      x: 0,
      y: 0,
      width: canvasSize.value.width,
      height: canvasSize.value.height,
    }
    const tmp = document.createElement('canvas')
    tmp.width = rect.width
    tmp.height = rect.height
    const ctx = tmp.getContext('2d', { willReadFrequently: true })
    if (!ctx) return { ok: false, error: 'context-unavailable' }

    ctx.translate(-rect.x, -rect.y)
    renderEditorCanvas(ctx, sourceImg.value, imgOffset, [...layers], canvasSize.value, bgColor.value, null, layerImages, undefined, imgAlpha.value, showTransparencyGrid)
    return { ok: true, value: tmp }
  }

  function encodeCanvas(canvas: HTMLCanvasElement, mimeType: string, quality?: number): Promise<EditorExportResult<Blob>> {
    return new Promise((resolve) => {
      canvas.toBlob(
        (blob) => {
          resolve(blob ? { ok: true, value: blob } : { ok: false, error: 'encode-failed' })
        },
        mimeType,
        quality
      )
    })
  }

  async function renderPng(selection?: EditorExportRect): Promise<EditorExportResult<Blob>> {
    const rendered = renderToTempCanvas(selection, false)
    if (!rendered.ok) return rendered
    return encodeCanvas(rendered.value, 'image/png')
  }

  function download(format: 'png' | 'jpeg' | 'webp' = 'png') {
    const rendered = renderToTempCanvas()
    if (!rendered.ok) return
    const mimeType = format === 'jpeg' ? 'image/jpeg' : format === 'webp' ? 'image/webp' : 'image/png'
    const quality = format === 'jpeg' ? 0.92 : format === 'webp' ? 0.9 : undefined
    const ext = format === 'jpeg' ? 'jpg' : format
    rendered.value.toBlob(
      (blob) => {
        if (!blob) return
        const url = URL.createObjectURL(blob)
        const link = document.createElement('a')
        link.download = `edited-${Date.now()}.${ext}`
        link.href = url
        link.click()
        URL.revokeObjectURL(url)
      },
      mimeType,
      quality
    )
  }

  async function copyToClipboard() {
    const rendered = renderToTempCanvas()
    if (!rendered.ok) return
    const result = await encodeCanvas(rendered.value, 'image/png')
    if (!result.ok) return
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': result.value })])
  }

  async function getPreviewBlobUrl(): Promise<string> {
    const rendered = renderToTempCanvas()
    if (!rendered.ok) throw new Error('Failed to render preview')
    const result = await encodeCanvas(rendered.value, 'image/png')
    if (!result.ok) throw new Error('Failed to render preview')
    return URL.createObjectURL(result.value)
  }

  return { renderToTempCanvas, renderPng, download, copyToClipboard, getPreviewBlobUrl }
}
