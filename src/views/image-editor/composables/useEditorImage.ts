import type { Ref } from 'vue'
import type { EditorLayer, EditorRasterSource, EditorTool } from '../utils/types'
import type { ImageEditorDoc } from './useImageEditorDoc'
import { resetLayerCounter } from './useImageEditorLayers'
import { serializeEditorImage } from '../utils/image-resource'

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

  function loadImage(file: File): Promise<void> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader()
      reader.onload = () => loadImageFromUrl(reader.result as string).then(resolve, reject)
      reader.onerror = () => reject(reader.error)
      reader.readAsDataURL(file)
    })
  }

  function loadImageFromUrl(url: string): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      const img = new Image()
      img.crossOrigin = 'anonymous'
      img.onload = () => {
        let serializedSource: string | undefined
        if (persistSourceImage) {
          try {
            serializedSource = serializeEditorImage(img)
          } catch (error) {
            reject(error)
            return
          }
        }
        pushUndo()
        sourceImg.value = img
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
