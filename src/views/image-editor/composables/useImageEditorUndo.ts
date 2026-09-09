import * as Y from 'yjs'
import { shallowRef } from 'vue'
import type { ImageEditorDoc } from './useImageEditorDoc'

export function useImageEditorUndo(doc: ImageEditorDoc) {
  const defaultCaptureTimeout = 0
  const undoManager = new Y.UndoManager(
    [doc.yLayers, doc.meta, doc.yImages],
    {
      trackedOrigins: new Set([null]),
      captureTimeout: defaultCaptureTimeout,
    },
  )
  let gestureActive = false
  let gestureUndoStackLength = 0
  let gestureRedoStack: typeof undoManager.redoStack = []

  const canUndo = shallowRef(undoManager.undoStack.length > 0)
  const canRedo = shallowRef(undoManager.redoStack.length > 0)

  function refresh() {
    canUndo.value = undoManager.undoStack.length > 0
    canRedo.value = undoManager.redoStack.length > 0
  }

  undoManager.on('stack-item-added', refresh)
  undoManager.on('stack-item-popped', refresh)
  undoManager.on('stack-cleared', refresh)

  function undo() {
    if (undoManager.undoStack.length === 0) return
    undoManager.undo()
  }

  function redo() {
    if (undoManager.redoStack.length === 0) return
    undoManager.redo()
  }

  function pushUndo() {
    undoManager.stopCapturing()
  }

  function beginGesture() {
    undoManager.stopCapturing()
    gestureUndoStackLength = undoManager.undoStack.length
    gestureRedoStack = [...undoManager.redoStack]
    undoManager.captureTimeout = Number.POSITIVE_INFINITY
    gestureActive = true
  }

  function endGesture() {
    if (!gestureActive) return
    gestureActive = false
    undoManager.captureTimeout = defaultCaptureTimeout
    undoManager.stopCapturing()
    gestureRedoStack = []
  }

  function cancelGesture() {
    if (!gestureActive) return
    gestureActive = false
    undoManager.captureTimeout = defaultCaptureTimeout
    undoManager.stopCapturing()
    while (undoManager.undoStack.length > gestureUndoStackLength) {
      undoManager.undo()
    }
    undoManager.redoStack.splice(0, undoManager.redoStack.length, ...gestureRedoStack)
    gestureRedoStack = []
    refresh()
  }

  function clearHistory() {
    undoManager.clear()
  }

  function dispose() {
    endGesture()
    undoManager.off('stack-item-added', refresh)
    undoManager.off('stack-item-popped', refresh)
    undoManager.off('stack-cleared', refresh)
    undoManager.destroy()
  }

  return {
    undoManager,
    canUndo,
    canRedo,
    undo,
    redo,
    pushUndo,
    beginGesture,
    endGesture,
    cancelGesture,
    clearHistory,
    dispose,
  }
}

export type ImageEditorUndo = ReturnType<typeof useImageEditorUndo>
