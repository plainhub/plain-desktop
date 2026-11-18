import type { Ref } from 'vue'
import { EditorLayerType, EditorFontWeight, EditorFontStyle, type EditorLayer, type CanvasSize, type StickerLayer } from '@/views/image-editor/utils/types'
import { measureStickerSize } from '@/views/image-editor/utils/editor-draw-layers'
import type { ImageEditorDoc } from './useImageEditorDoc'
import { generateRecordId } from '@/lib/record-id.generated'

const STICKER_COLORS = ['#fef08a', '#bbf7d0', '#bfdbfe', '#fbcfe8', '#fde68a']
let _stickerColorIdx = 0

export function useImageEditorSticker(
  layers: EditorLayer[],
  canvasSize: Ref<CanvasSize>,
  selectedLayerId: Ref<string | null>,
  doc: ImageEditorDoc,
  pushUndo: () => void,
  nextLayerName: (type: string) => string,
) {
  function createStickerLayer(x: number, y: number): StickerLayer {
    const color = STICKER_COLORS[_stickerColorIdx % STICKER_COLORS.length]!
    _stickerColorIdx++
    return {
      id: generateRecordId(), type: EditorLayerType.STICKER, visible: true,
      name: nextLayerName('Note'),
      x, y,
      w: canvasSize.value.width * 0.18,
      h: canvasSize.value.width * 0.06,
      color,
      text: 'Note',
      fontSize: Math.round(canvasSize.value.width * 0.02),
      fontWeight: EditorFontWeight.SEMIBOLD,
      fontStyle: EditorFontStyle.NORMAL,
      rotation: 0,
    }
  }

  function addStickerLayer(x?: number, y?: number) {
    if (layers.length >= 10) return
    pushUndo()
    const cx = x ?? canvasSize.value.width / 2
    const cy = y ?? canvasSize.value.height / 2
    const layer = createStickerLayer(cx, cy)
    doc.addLayer(layer)
    selectedLayerId.value = layer.id
  }

  function autoResizeSticker(layer: StickerLayer) {
    const { w, h } = measureStickerSize(layer)
    doc.setLayerProps(layer.id, { w, h })
  }

  function updateStickerText(layer: StickerLayer, text: string) {
    doc.setLayerProp(layer.id, 'text', text)
    const updated = { ...layer, text }
    const { w, h } = measureStickerSize(updated)
    doc.setLayerProps(layer.id, { w, h })
  }

  function updateStickerFontSize(layer: StickerLayer, size: number) {
    doc.setLayerProp(layer.id, 'fontSize', size)
    const updated = { ...layer, fontSize: size }
    const { w, h } = measureStickerSize(updated)
    doc.setLayerProps(layer.id, { w, h })
  }

  function toggleStickerBold(layer: StickerLayer) {
    doc.setLayerProp(layer.id, 'fontWeight', layer.fontWeight === EditorFontWeight.BOLD ? EditorFontWeight.SEMIBOLD : EditorFontWeight.BOLD)
  }

  function toggleStickerItalic(layer: StickerLayer) {
    doc.setLayerProp(layer.id, 'fontStyle', layer.fontStyle === EditorFontStyle.ITALIC ? EditorFontStyle.NORMAL : EditorFontStyle.ITALIC)
  }

  return {
    createStickerLayer,
    addStickerLayer,
    autoResizeSticker,
    updateStickerText,
    updateStickerFontSize,
    toggleStickerBold,
    toggleStickerItalic,
  }
}
