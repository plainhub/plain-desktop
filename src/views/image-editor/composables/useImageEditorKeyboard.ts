import { onMounted, onUnmounted, nextTick, type Ref } from 'vue'
import type { EditorLayer } from '@/views/image-editor/utils/types'
import { isEditorTextLayer, isEditorImageLayer } from '@/views/image-editor/utils/types'
import type { ImageEditorDoc } from './useImageEditorDoc'
import { shortUUID } from '@/lib/strutil'
import { copyTextToClipboard } from '@/lib/clipboard'

interface KeyboardCallbacks {
  removeLayer: (idx: number) => void
  addTextLayer: (x?: number, y?: number) => void
  addImageLayerFromFile: (file: File) => Promise<void>
  pushUndo: () => void
  draw: () => void
}

export function useImageEditorKeyboard(
  layers: EditorLayer[],
  selectedLayerId: Ref<string | null>,
  inlineEditLayerId: Ref<string | null>,
  doc: ImageEditorDoc,
  ops: KeyboardCallbacks,
) {
  let clipboardLayer: EditorLayer | null = null
  let clipboardImageSrc: string | null = null

  function onKeyDown(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement).tagName
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return
    if (inlineEditLayerId.value) return

    if (e.key === 'Delete' || e.key === 'Backspace') {
      if (!selectedLayerId.value) return
      const idx = layers.findIndex(l => l.id === selectedLayerId.value)
      if (idx !== -1) {
        e.preventDefault()
        ops.removeLayer(idx)
        selectedLayerId.value = null
      }
      return
    }

    const isMod = e.metaKey || e.ctrlKey

    if (isMod && e.key === 'c') {
      if (!selectedLayerId.value) return
      const layer = layers.find(l => l.id === selectedLayerId.value)
      if (!layer) return
      clipboardLayer = JSON.parse(JSON.stringify(layer))
      if (isEditorImageLayer(layer)) {
        clipboardImageSrc = doc.getImageSrc(layer.id) ?? null
      }
      if (isEditorTextLayer(layer)) {
        copyTextToClipboard(layer.text)
      }
    }
  }

  function onPaste(e: ClipboardEvent) {
    const tag = (e.target as HTMLElement)?.tagName
    if (tag === 'INPUT' || tag === 'TEXTAREA') return
    if (inlineEditLayerId.value) return
    if (layers.length >= 10) return

    const items = e.clipboardData?.items
    if (items) {
      for (const item of Array.from(items)) {
        if (item.type.startsWith('image/')) {
          const file = item.getAsFile()
          if (file) {
            e.preventDefault()
            ops.addImageLayerFromFile(file)
            return
          }
        }
      }
    }

    if (clipboardLayer) {
      e.preventDefault()
      ops.pushUndo()
      const clone = JSON.parse(JSON.stringify(clipboardLayer)) as EditorLayer
      clone.id = shortUUID()
      if ('x' in clone) (clone as any).x += 20
      if ('y' in clone) (clone as any).y += 20
      if ('x1' in clone) { (clone as any).x1 += 20; (clone as any).y1 += 20; (clone as any).x2 += 20; (clone as any).y2 += 20 }
      if ('cx' in clone) { (clone as any).cx += 20; (clone as any).cy += 20 }
      doc.addLayer(clone)
      if (isEditorImageLayer(clone) && clipboardImageSrc) {
        doc.setImageSrc(clone.id, clipboardImageSrc)
      }
      selectedLayerId.value = clone.id
      nextTick(() => ops.draw())
      return
    }

    const text = e.clipboardData?.getData('text/plain')
    if (text?.trim()) {
      e.preventDefault()
      ops.addTextLayer()
      const newLayer = layers[layers.length - 1]
      if (newLayer && isEditorTextLayer(newLayer)) {
        doc.setLayerProp(newLayer.id, 'text', text.trim().slice(0, 200))
      }
      selectedLayerId.value = newLayer?.id ?? null
      nextTick(() => ops.draw())
    }
  }

  onMounted(() => {
    document.addEventListener('keydown', onKeyDown)
    document.addEventListener('paste', onPaste)
  })
  onUnmounted(() => {
    document.removeEventListener('keydown', onKeyDown)
    document.removeEventListener('paste', onPaste)
  })
}
