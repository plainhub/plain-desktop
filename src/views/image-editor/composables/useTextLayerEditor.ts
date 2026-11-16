import { ref, computed, watch } from 'vue'
import { useEditorPopup } from './useEditorPopup'
import { useI18n } from 'vue-i18n'
import type { EditorTextLayer } from '@/views/image-editor/utils/types'
import type { ImageEditorDoc } from './useImageEditorDoc'

interface ShadowPreset { label: string; offsetX: number; offsetY: number; blur: number; color: string }

export function useTextLayerEditor(
  layerRef: () => EditorTextLayer | null,
  rectRef: () => { top: number; left: number },
  doc: ImageEditorDoc,
  emitClose: () => void,
) {
  const { t } = useI18n()
  const { panelRef, posX, posY, onDragStart } = useEditorPopup(() => !!layerRef(), rectRef, 320, emitClose)

  const showStroke = ref(false)
  const showShadow = ref(false)
  const showMore = ref(false)

  watch(() => layerRef()?.id, () => {
    const layer = layerRef()
    if (layer) {
      showStroke.value = layer.textStroke.width > 0
      showShadow.value = layer.textShadow.blur > 0 || layer.textShadow.offsetX !== 0 || layer.textShadow.offsetY !== 0
      showMore.value = layer.rotation !== 0 || layer.letterSpacing !== 0
    }
  }, { immediate: true })

  const hasShadow = computed(() => {
    const s = layerRef()?.textShadow
    return s ? s.blur > 0 || s.offsetX !== 0 || s.offsetY !== 0 : false
  })

  const shadowPresets = computed<ShadowPreset[]>(() => [
    { label: t('image_editor.shadow_off'), offsetX: 0, offsetY: 0, blur: 0, color: 'rgba(0,0,0,0.7)' },
    { label: t('image_editor.shadow_soft'), offsetX: 0, offsetY: 0, blur: 12, color: 'rgba(0,0,0,0.7)' },
    { label: t('image_editor.shadow_hard'), offsetX: 4, offsetY: 4, blur: 0, color: '#000000' },
    { label: t('image_editor.shadow_drop'), offsetX: 6, offsetY: 6, blur: 8, color: 'rgba(0,0,0,0.6)' },
  ])

  function setProperty<K extends keyof EditorTextLayer>(key: K, value: EditorTextLayer[K]) {
    doc.setLayerProp(layerRef()!.id, key, value)
  }

  function setStroke<K extends keyof EditorTextLayer['textStroke']>(key: K, value: EditorTextLayer['textStroke'][K]) {
    doc.setLayerMapProp(layerRef()!.id, 'textStroke', key, value)
  }

  function setShadow<K extends keyof EditorTextLayer['textShadow']>(key: K, value: EditorTextLayer['textShadow'][K]) {
    doc.setLayerMapProp(layerRef()!.id, 'textShadow', key, value)
  }

  function applyShadowPreset(preset: ShadowPreset) {
    doc.ydoc.transact(() => {
      for (const key of ['offsetX', 'offsetY', 'blur', 'color'] as const) setShadow(key, preset[key])
    })
  }

  function isShadowPresetActive(preset: ShadowPreset): boolean {
    const s = layerRef()?.textShadow
    return s ? s.offsetX === preset.offsetX && s.offsetY === preset.offsetY && s.blur === preset.blur : false
  }

  return {
    panelRef, posX, posY, onDragStart,
    showStroke, showShadow, showMore, hasShadow,
    shadowPresets, applyShadowPreset, isShadowPresetActive, setProperty, setStroke, setShadow,
  }
}
