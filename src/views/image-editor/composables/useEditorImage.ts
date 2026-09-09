import type { Ref } from 'vue'
import type { EditorLayer, EditorRasterSource, EditorTool } from '../utils/types'
import type { ImageEditorDoc } from './useImageEditorDoc'
import { resetLayerCounter } from './useImageEditorLayers'

export interface EditorImageContext {
  doc: ImageEditorDoc
  sourceImg: Ref<EditorRasterSource | null>
  selectedLayerId: Ref<string | null>
  activeTool: Ref<EditorTool>
  previewLayer: Ref<EditorLayer | null>
  editorActive: Ref<boolean>
  pushUndo: () => void
  requestRender: () => void
  clearHistory: () => void
  persistSourceImage?: (value: string | null) => void
  onReset?: () => void
}

export function useEditorImage(ctx: EditorImageContext) {
  const {
    doc, sourceImg, selectedLayerId, activeTool, previewLayer, editorActive,
    pushUndo, requestRender, clearHistory, persistSourceImage, onReset,
  } = ctx

  function loadImage(file: File) {
    return new Promise<void>((resolve, reject) => {
      const blobUrl = URL.createObjectURL(file)
      const img = new Image()
      img.onload = () => {
        pushUndo()
        sourceImg.value = img
        doc.ydoc.transact(() => {
          doc.setCanvasSize(img.naturalWidth, img.naturalHeight)
          doc.setImgOffset(0, 0)
          doc.clearLayers()
        })
        selectedLayerId.value = null; resetLayerCounter()
        editorActive.value = true
        requestRender()
        resolve()
        if (persistSourceImage) {
          const reader = new FileReader()
          reader.onload = () => {
            persistSourceImage(reader.result as string)
          }
          reader.readAsDataURL(file)
        }
      }
      img.onerror = () => {
        URL.revokeObjectURL(blobUrl)
        reject(new Error('Failed to load image'))
      }
      img.src = blobUrl
    })
  }

  function loadImageFromUrl(url: string): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      const img = new Image()
      img.onload = () => {
        pushUndo()
        sourceImg.value = img
        let serializedSource: string | undefined
        if (persistSourceImage) {
          serializedSource = url
          try {
            const tmp = document.createElement('canvas')
            tmp.width = img.naturalWidth
            tmp.height = img.naturalHeight
            tmp.getContext('2d')!.drawImage(img, 0, 0)
            serializedSource = tmp.toDataURL('image/png')
          } catch {
            // Canvas tainted (cross-origin without CORS) — fall back to URL
          }
        }
        doc.ydoc.transact(() => {
          if (serializedSource) persistSourceImage?.(serializedSource)
          doc.setCanvasSize(img.naturalWidth, img.naturalHeight)
          doc.setImgOffset(0, 0)
          doc.clearLayers()
        })
        selectedLayerId.value = null; resetLayerCounter()
        editorActive.value = true
        requestRender()
        resolve()
      }
      img.onerror = () => reject(new Error('Failed to load image'))
      img.src = url
    })
  }

  function startBlank() {
    pushUndo()
    doc.ydoc.transact(() => {
      persistSourceImage?.(null)
      doc.setCanvasSize(1920, 1080)
      doc.setImgOffset(0, 0)
      doc.setBgColor('#ffffff')
      doc.clearLayers()
    })
    selectedLayerId.value = null; resetLayerCounter()
    editorActive.value = true
    requestRender()
  }

  function reset() {
    doc.ydoc.transact(() => {
      persistSourceImage?.(null)
      doc.setCanvasSize(1920, 1080)
      doc.setImgOffset(0, 0)
      doc.setBgColor('transparent')
      doc.clearLayers()
    })
    clearHistory()
    activeTool.value = 'select'
    previewLayer.value = null; selectedLayerId.value = null; resetLayerCounter()
    editorActive.value = false
    onReset?.()
  }

  function setBgColor(color: string) {
    pushUndo()
    doc.setBgColor(color)
  }

  function setSourceImg(img: EditorRasterSource | null) {
    if (img) {
      sourceImg.value = img
      persistSourceImage?.('src' in img ? img.src : img.toDataURL('image/png'))
    } else {
      sourceImg.value = null
      persistSourceImage?.(null)
    }
  }

  return { loadImage, loadImageFromUrl, startBlank, reset, setBgColor, setSourceImg }
}
