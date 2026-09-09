import type { Ref } from 'vue'
import type {
  EditorLayer, EditorTool, CanvasSize, EditorTextLayer, EditorImageLayer,
} from '@/views/image-editor/utils/types'
import type { ImageEditorDoc } from './useImageEditorDoc'
import { shortUUID } from '@/lib/strutil'

let _layerCounter = 0
export function resetLayerCounter() { _layerCounter = 0 }
function nextLayerName(type: string) { return `${type} ${++_layerCounter}` }
export { nextLayerName }

export function useImageEditorLayers(
  layers: EditorLayer[],
  layerImages: Map<string, HTMLImageElement>,
  canvasSize: Ref<CanvasSize>,
  selectedLayerId: Ref<string | null>,
  activeColor: Ref<string>,
  activeLineWidth: Ref<number>,
  activeFontSize: Ref<number>,
  doc: ImageEditorDoc,
  pushUndo: () => void,
) {
  function createRichTextLayer(x: number, y: number, text = 'Text', name = nextLayerName('Text')): EditorTextLayer {
    return {
      id: shortUUID(), type: 'text', visible: true, name,
      x, y, text,
      fontSize: activeFontSize.value, color: activeColor.value,
      fontFamily: 'Inter, sans-serif', fontWeight: 'bold', fontStyle: 'normal',
      textAlign: 'center', letterSpacing: 0, textDecoration: 'none',
      textStroke: { width: 0, color: '#000000' },
      textShadow: { offsetX: 0, offsetY: 0, blur: 0, color: 'rgba(0,0,0,0.7)' },
      glow: false, rotation: 0, maxWidth: canvasSize.value.width * 0.6,
    }
  }

  function addTextLayer(x?: number, y?: number) {
    if (layers.length >= 10) return
    pushUndo()
    const cx = x ?? canvasSize.value.width / 2
    const cy = y ?? canvasSize.value.height / 2
    const layer = createRichTextLayer(cx, cy)
    doc.addLayer(layer)
    selectedLayerId.value = layer.id
  }

  function loadImageFile(file: File): Promise<{ src: string; img: HTMLImageElement }> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader()
      reader.onload = () => {
        const src = reader.result as string
        const img = new Image()
        img.onload = () => resolve({ src, img })
        img.onerror = reject
        img.src = src
      }
      reader.onerror = reject
      reader.readAsDataURL(file)
    })
  }

  async function addImageLayerFromFile(file: File) {
    if (layers.length >= 10) return
    const { img, src } = await loadImageFile(file)
    pushUndo()
    const maxDim = Math.min(canvasSize.value.width, canvasSize.value.height) * 0.4
    const scale = Math.min(1, maxDim / Math.max(img.naturalWidth, img.naturalHeight))
    const w = Math.round(img.naturalWidth * scale)
    const h = Math.round(img.naturalHeight * scale)
    const layer: EditorImageLayer = {
      id: shortUUID(), type: 'image', visible: true, name: nextLayerName('Image'),
      x: canvasSize.value.width / 2, y: canvasSize.value.height / 2,
      w, h, opacity: 1, rotation: 0,
    }
    layerImages.set(layer.id, img)
    doc.addLayer(layer)
    doc.setImageSrc(layer.id, src)
    selectedLayerId.value = layer.id
  }

  async function replaceImageLayerFile(layerId: string, file: File) {
    const { img, src } = await loadImageFile(file)
    layerImages.set(layerId, img)
    doc.setImageSrc(layerId, src)
    const layer = layers.find(l => l.id === layerId && l.type === 'image') as EditorImageLayer | undefined
    if (layer) {
      const maxDim = Math.min(canvasSize.value.width, canvasSize.value.height) * 0.4
      const scale = Math.min(1, maxDim / Math.max(img.naturalWidth, img.naturalHeight))
      doc.setLayerProps(layerId, {
        w: Math.round(img.naturalWidth * scale),
        h: Math.round(img.naturalHeight * scale),
      })
    }
  }

  function createLayerFromDrag(tool: EditorTool, start: { x: number; y: number }, end: { x: number; y: number }): EditorLayer {
    const id = shortUUID()
    switch (tool) {
      case 'arrow':
        return { id, type: 'arrow', visible: true, name: nextLayerName('Arrow'), x1: start.x, y1: start.y, x2: end.x, y2: end.y, color: activeColor.value, lineWidth: activeLineWidth.value, rotation: 0 }
      case 'rect':
        return { id, type: 'rect', visible: true, name: nextLayerName('Rect'), x: Math.min(start.x, end.x), y: Math.min(start.y, end.y), w: Math.abs(end.x - start.x), h: Math.abs(end.y - start.y), color: activeColor.value, lineWidth: activeLineWidth.value }
      case 'ellipse':
        return { id, type: 'ellipse', visible: true, name: nextLayerName('Ellipse'), cx: (start.x + end.x) / 2, cy: (start.y + end.y) / 2, rx: Math.abs(end.x - start.x) / 2, ry: Math.abs(end.y - start.y) / 2, color: activeColor.value, lineWidth: activeLineWidth.value }
      case 'highlight':
        return { id, type: 'highlight', visible: true, name: nextLayerName('Highlight'), x: Math.min(start.x, end.x), y: Math.min(start.y, end.y), w: Math.abs(end.x - start.x), h: Math.abs(end.y - start.y), color: activeColor.value }
      case 'mosaic':
        return { id, type: 'mosaic', visible: true, name: nextLayerName('Mosaic'), x: Math.min(start.x, end.x), y: Math.min(start.y, end.y), w: Math.abs(end.x - start.x), h: Math.abs(end.y - start.y), blockSize: 12 }
      default:
        return { id, type: 'rect', visible: true, name: nextLayerName('Rect'), x: start.x, y: start.y, w: 0, h: 0, color: activeColor.value, lineWidth: activeLineWidth.value }
    }
  }

  function getBoundsSize(layer: EditorLayer): number {
    if (layer.type === 'arrow') return Math.hypot(layer.x2 - layer.x1, layer.y2 - layer.y1)
    if (layer.type === 'ellipse') return layer.rx * 2 + layer.ry * 2
    if (layer.type === 'freehand') return layer.points.length > 2 ? 10 : 0
    return (layer as any).w + (layer as any).h
  }

  function removeLayer(idx: number) {
    pushUndo()
    const removed = doc.removeLayer(idx)
    if (removed) {
      if (removed.id === selectedLayerId.value) selectedLayerId.value = null
      layerImages.delete(removed.id)
    }
  }

  function reorderLayer(from: number, to: number) {
    if (from === to) return; pushUndo()
    doc.moveLayer(from, to)
  }

  function toggleLayerVisibility(id: string) {
    const layer = layers.find(l => l.id === id)
    if (layer) { pushUndo(); doc.setLayerProp(id, 'visible', !layer.visible) }
  }

  function clearLayers() {
    if (!layers.length) return; pushUndo(); doc.clearLayers(); selectedLayerId.value = null
  }

  return {
    createRichTextLayer, addTextLayer,
    addImageLayerFromFile, replaceImageLayerFile,
    createLayerFromDrag, getBoundsSize,
    removeLayer, reorderLayer, toggleLayerVisibility, clearLayers,
  }
}
