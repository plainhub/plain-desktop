import { cssFontWeight } from '@/views/image-editor/utils/text-styles'
import { ref, computed, nextTick, type Ref } from 'vue'
import {
  EditorLayerType, EditorTextAlign, type CanvasSize, type EditorLayer, type EditorTextLayer,
  type StickerLayer, isEditorTextLayer,
} from '@/views/image-editor/utils/types'
import { STICKER_PALETTE } from '@/views/image-editor/utils/editor-draw-layers'
import type { ImageEditorDoc } from './useImageEditorDoc'

interface InlineEditCallbacks {
  draw: () => void
}

export function useImageEditorInlineEdit(
  layers: EditorLayer[],
  wrapRef: Ref<HTMLElement | null>,
  canvasSize: Ref<CanvasSize>,
  selectedLayerId: Ref<string | null>,
  inlineEditLayerId: Ref<string | null>,
  doc: ImageEditorDoc,
  callbacks: InlineEditCallbacks,
) {
  const inlineEditRef = ref<HTMLTextAreaElement | null>(null)
  const inlineEditPositionStyle = ref<Record<string, string>>({})

  const inlineEditLayer = computed(() => {
    if (!inlineEditLayerId.value) return null
    const l = layers.find(l => l.id === inlineEditLayerId.value)
    if (!l) return null
    if (isEditorTextLayer(l)) return l
    if (l.type === EditorLayerType.STICKER) return l as StickerLayer
    return null
  })

  function startInlineEdit(layerId: string) {
    const layer = layers.find(l => l.id === layerId)
    if (!layer) return
    const isText = isEditorTextLayer(layer)
    const isSticker = layer.type === EditorLayerType.STICKER
    if (!isText && !isSticker) return
    const wrap = wrapRef.value
    if (!wrap) return

    selectedLayerId.value = layerId
    inlineEditLayerId.value = layerId
    callbacks.draw()

    const scale = wrap.clientWidth / canvasSize.value.width

    if (isSticker) {
      const sl = layer as StickerLayer
      const padding = 14
      const wDisplay = sl.w * scale
      const hDisplay = sl.h * scale
      const fontSize = Math.round(sl.fontSize * scale * 10) / 10
      const left = (sl.x - sl.w / 2) * scale
      const top = (sl.y - sl.h / 2) * scale
      const palette = STICKER_PALETTE[sl.color] ?? { bg: sl.color, text: '#374151' }
      const transforms: string[] = []
      if (sl.rotation) transforms.push(`rotate(${sl.rotation}deg)`)
      inlineEditPositionStyle.value = {
        left: `${left}px`,
        top: `${top}px`,
        width: `${wDisplay}px`,
        'min-height': `${hDisplay}px`,
        padding: `${padding * scale}px`,
        transform: transforms.length ? transforms.join(' ') : 'none',
        'transform-origin': '50% 50%',
        'font-size': `${fontSize}px`,
        'font-family': 'Inter, sans-serif',
        'font-weight': cssFontWeight(sl.fontWeight),
        'font-style': sl.fontStyle.toLowerCase(),
        color: palette.text,
        'background-color': palette.bg,
        'border-radius': `${8 * scale}px`,
        'text-align': 'left',
        'line-height': '1.3',
        'caret-color': palette.text,
        'box-sizing': 'border-box',
        overflow: 'hidden',
      }
    } else {
      const tl = layer as EditorTextLayer

      const maxWDisplay = tl.maxWidth * scale
      const fontSize = Math.round(tl.fontSize * scale * 10) / 10
      const strokeW = Math.round(tl.textStroke.width * scale * 10) / 10

      const charW = tl.fontSize * 0.6
      const lineCount = Math.max(1, Math.ceil((tl.text.length * charW) / tl.maxWidth))
      const totalH = lineCount * tl.fontSize * 1.15
      const totalHDisplay = totalH * scale

      const centerXDisplay = tl.x * scale
      const centerYDisplay = tl.y * scale

      let left: number
      if (tl.textAlign === EditorTextAlign.CENTER) left = centerXDisplay - maxWDisplay / 2
      else if (tl.textAlign === EditorTextAlign.RIGHT) left = centerXDisplay - maxWDisplay
      else left = centerXDisplay - maxWDisplay / 2

      const top = centerYDisplay - totalHDisplay / 2
      const transforms: string[] = []
      if (tl.rotation) transforms.push(`rotate(${tl.rotation}deg)`)

      let originX = '50%'
      if (tl.textAlign === EditorTextAlign.LEFT) originX = '0%'
      else if (tl.textAlign === EditorTextAlign.RIGHT) originX = '100%'

      let textShadow = 'none'
      if (tl.glow) {
        textShadow = `0 0 ${Math.round((tl.textShadow.blur || 20) * scale)}px ${tl.color}`
      } else if (tl.textShadow.blur > 0 || tl.textShadow.offsetX !== 0 || tl.textShadow.offsetY !== 0) {
        textShadow = `${Math.round(tl.textShadow.offsetX * scale)}px ${Math.round(tl.textShadow.offsetY * scale)}px ${Math.round(tl.textShadow.blur * scale)}px ${tl.textShadow.color}`
      }

      inlineEditPositionStyle.value = {
        left: `${left}px`,
        top: `${top}px`,
        width: `${maxWDisplay}px`,
        transform: transforms.length ? transforms.join(' ') : 'none',
        'transform-origin': `${originX} 50%`,
        'font-size': `${fontSize}px`,
        'font-family': tl.fontFamily,
        'font-weight': cssFontWeight(tl.fontWeight),
        'font-style': tl.fontStyle.toLowerCase(),
        color: tl.color,
        'text-align': tl.textAlign.toLowerCase(),
        '-webkit-text-stroke': strokeW > 0 ? `${strokeW}px ${tl.textStroke.color}` : '',
        'paint-order': 'stroke fill',
        'line-height': '1.15',
        'letter-spacing': tl.letterSpacing ? `${tl.letterSpacing * scale}px` : '0px',
        'caret-color': '#3b82f6',
        'text-shadow': textShadow,
      }
    }

    nextTick(() => {
      const el = inlineEditRef.value
      if (el) {
        autoResizeTextarea(el)
        el.focus()
        el.select()
      }
    })
  }

  function commitInlineEdit() {
    if (!inlineEditLayerId.value) return
    inlineEditLayerId.value = null
    nextTick(() => callbacks.draw())
  }

  function onInlineEditInput(e: Event) {
    const el = e.target as HTMLTextAreaElement
    const layer = inlineEditLayer.value
    if (layer && 'text' in layer) doc.setLayerProp(layer.id, 'text', el.value)
    autoResizeTextarea(el)
  }

  function autoResizeTextarea(el: HTMLTextAreaElement) {
    el.style.height = 'auto'
    el.style.height = el.scrollHeight + 'px'
  }

  return {
    inlineEditLayer,
    inlineEditRef,
    inlineEditPositionStyle,
    startInlineEdit,
    commitInlineEdit,
    onInlineEditInput,
  }
}
