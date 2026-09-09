import { onMounted, onUnmounted, watch } from 'vue'
import { renderEditorCanvas } from '@/views/image-editor/utils/renderer'
import { PlainAppProjectStore } from '../store/plain-app-store'
import { EventSyncTransport } from '../sync/event-sync-transport'
import { useAnnotationSession, type AnnotationSession } from './useAnnotationSession'
import { useImageEditorPersistence } from './useImageEditorPersistence'

/**
 * Full image-editor adapter around the reusable in-memory annotation session.
 * Project persistence, URL mutation, and network synchronization stay here.
 */
export function useImageEditorCore() {
  let session!: AnnotationSession
  let scheduleSaveAdapter: () => void = () => {}
  let clearProjectAdapter: () => void = () => {}

  session = useAnnotationSession({
    allowSourceMove: true,
    allowDestructiveCrop: true,
    textCommitMode: 'immediate',
    persistSourceImage: (value) => session.doc.setSourceImage(value),
    onReset: () => clearProjectAdapter(),
  })

  const { canvasRef, overlayRef, wrapRef, doc, binding, state, tools } = session
  const { sourceImg, imgOffset, canvasSize, bgColor, imgAlpha, editorActive } = state
  const { isCropping } = session.crop
  const store = new PlainAppProjectStore()

  function makeThumbnail(): string | null {
    try {
      const maxDimension = 200
      const { width, height } = canvasSize.value
      const scale = Math.min(maxDimension / width, maxDimension / height, 1)
      const thumbnail = document.createElement('canvas')
      thumbnail.width = Math.round(width * scale)
      thumbnail.height = Math.round(height * scale)
      const context = thumbnail.getContext('2d', { willReadFrequently: true })!
      context.scale(scale, scale)
      renderEditorCanvas(context, sourceImg.value, imgOffset, [...state.layers], canvasSize.value, bgColor.value, null, state.layerImages, undefined, imgAlpha.value)
      return thumbnail.toDataURL('image/jpeg', 0.6)
    } catch {
      return null
    }
  }

  function onRestored() {
    const dataUrl = doc.getSourceImage()
    if (dataUrl) {
      const image = new Image()
      image.onload = () => {
        sourceImg.value = image
        session.history.clearHistory()
        session.render.draw()
      }
      image.onerror = () => {
        session.history.clearHistory()
        session.render.draw()
      }
      image.src = dataUrl
    } else {
      session.history.clearHistory()
      session.render.draw()
    }
  }

  const persistence = useImageEditorPersistence(doc, store, editorActive, makeThumbnail, onRestored)
  const { projectId, scheduleSave, flushSave, tryRestore, ensureProjectId, clearProject, deleteProject, listRecentProjects, loadProjectById } = persistence
  scheduleSaveAdapter = scheduleSave
  clearProjectAdapter = clearProject

  const transport = new EventSyncTransport(() => projectId.value)
  const onDocumentUpdate = (update: Uint8Array, origin: unknown) => {
    if (origin === 'remote' || origin === 'load') return
    transport.broadcastUpdate(update)
  }
  doc.ydoc.on('update', onDocumentUpdate)
  const stopTransportUpdates = transport.onUpdate((update) => doc.applyRemoteUpdate(update))
  const stopAutosave = watch(binding.syncVersion, () => scheduleSaveAdapter())

  function onKeyDown(event: KeyboardEvent) {
    const key = event.key.toLowerCase()
    if ((event.metaKey || event.ctrlKey) && event.shiftKey && key === 'z') {
      event.preventDefault()
      session.history.redo()
      return
    }
    if ((event.metaKey || event.ctrlKey) && key === 'z') {
      event.preventDefault()
      session.history.undo()
      return
    }
    if (event.key === 'Escape') {
      if (isCropping.value) {
        session.crop.cancelCrop()
        return
      }
      state.selectedLayerId.value = null
      tools.activeTool.value = 'select'
      session.render.draw()
    }
  }

  function onBeforeUnload() {
    flushSave()
  }

  onMounted(async () => {
    window.addEventListener('keydown', onKeyDown)
    window.addEventListener('beforeunload', onBeforeUnload)
    document.body.style.overflow = 'hidden'
    const restored = await tryRestore()
    if (!restored) ensureProjectId()
    await transport.connect()
    session.render.draw()
  })

  onUnmounted(() => {
    window.removeEventListener('keydown', onKeyDown)
    window.removeEventListener('beforeunload', onBeforeUnload)
    flushSave()
    document.body.style.overflow = ''
    document.body.style.top = ''
    document.body.style.left = ''
    document.body.style.right = ''
    stopAutosave()
    doc.ydoc.off('update', onDocumentUpdate)
    stopTransportUpdates()
    transport.destroy()
    session.dispose()
  })

  return {
    canvasRef,
    overlayRef,
    wrapRef,
    doc,
    binding,
    state,
    tools,
    crop: session.crop,
    history: {
      undo: session.history.undo,
      redo: session.history.redo,
      canUndo: session.history.canUndo,
      canRedo: session.history.canRedo,
      pushUndo: session.history.pushUndo,
    },
    render: { draw: session.render.draw, resizeCanvas: session.render.resizeCanvas },
    image: session.image,
    pointer: {
      onPointerDown: session.pointer.onPointerDown,
      onPointerMove: session.pointer.onPointerMove,
      onPointerUp: session.pointer.onPointerUp,
      onDoubleClick: session.pointer.onDoubleClick,
    },
    layerOps: session.layerOps,
    sticker: session.sticker,
    exportOps: {
      download: session.exportOps.download,
      copyToClipboard: session.exportOps.copyToClipboard,
      getPreviewBlobUrl: session.exportOps.getPreviewBlobUrl,
    },
    persistence: { scheduleSave, listRecentProjects, loadProjectById, deleteProject },
  }
}
