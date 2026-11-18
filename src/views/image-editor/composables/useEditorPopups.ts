import { ref, computed, watch, nextTick, type Ref } from 'vue'
import { EditorLayerType, isEditorTextLayer, type EditorTextLayer, type EditorLayer, type StickerLayer } from '@/views/image-editor/utils/types'

/**
 * Manages text and sticker style popup editors in the image editor.
 * Opens the appropriate popup when a layer is selected.
 */
export function useEditorPopups(
  layers: EditorLayer[],
  selectedLayerId: Ref<string | null>,
  wrapRef: Ref<HTMLElement | null>,
) {
  const popupLayerId = ref<string | null>(null)
  const popupRect = ref({ top: 0, left: 0 })

  const popupLayer = computed(() => {
    if (!popupLayerId.value) return null
    return layers.find(l => l.id === popupLayerId.value && isEditorTextLayer(l)) as EditorTextLayer | undefined ?? null
  })

  function openTextEditor(id: string) {
    const layer = layers.find(l => l.id === id)
    if (!layer || !isEditorTextLayer(layer)) return
    selectedLayerId.value = id
    popupLayerId.value = id
    nextTick(() => {
      const wrap = wrapRef.value
      if (wrap) {
        const r = wrap.getBoundingClientRect()
        popupRect.value = { top: Math.max(8, r.top), left: Math.min(r.right + 8, window.innerWidth - 328) }
      }
    })
  }

  const stickerPopupId = ref<string | null>(null)
  const stickerPopupRect = ref({ top: 0, left: 0 })

  const stickerPopupLayer = computed(() => {
    if (!stickerPopupId.value) return null
    const l = layers.find(l => l.id === stickerPopupId.value && l.type === EditorLayerType.STICKER)
    return (l as StickerLayer) ?? null
  })

  function openStickerEditor(id: string) {
    const layer = layers.find(l => l.id === id)
    if (!layer || layer.type !== EditorLayerType.STICKER) return
    selectedLayerId.value = id
    stickerPopupId.value = id
    nextTick(() => {
      const wrap = wrapRef.value
      if (wrap) {
        const r = wrap.getBoundingClientRect()
        stickerPopupRect.value = { top: Math.max(8, r.top), left: Math.min(r.right + 8, window.innerWidth - 264) }
      }
    })
  }

  function selectLayer(id: string | null) {
    selectedLayerId.value = id
    if (!id) { popupLayerId.value = null; stickerPopupId.value = null; return }
    const layer = layers.find(l => l.id === id)
    if (!layer) { popupLayerId.value = null; stickerPopupId.value = null; return }
    if (isEditorTextLayer(layer)) {
      stickerPopupId.value = null
      openTextEditor(id)
    } else if (layer.type === EditorLayerType.STICKER) {
      popupLayerId.value = null
      openStickerEditor(id)
    } else {
      popupLayerId.value = null
      stickerPopupId.value = null
    }
  }

  watch(selectedLayerId, selectLayer)

  return {
    popupLayerId, popupRect, popupLayer,
    stickerPopupId, stickerPopupRect, stickerPopupLayer,
    openTextEditor, openStickerEditor, selectLayer,
  }
}
