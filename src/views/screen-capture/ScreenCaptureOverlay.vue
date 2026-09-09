<template>
  <div
    ref="stageRef"
    class="screen-capture-overlay"
    data-testid="capture-stage"
    tabindex="-1"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="onPointerUp"
    @pointercancel="onPointerCancel"
    @dblclick="onDoubleClick"
    @contextmenu="onContextMenu"
  >
    <canvas ref="frozenCanvasRef" class="screen-capture-overlay__canvas screen-capture-overlay__source" :aria-label="messages.a11y.capturedScreen" />
    <canvas ref="canvasRef" class="screen-capture-overlay__canvas screen-capture-overlay__annotations" :aria-label="messages.a11y.annotations" />
    <canvas ref="overlayRef" class="screen-capture-overlay__canvas screen-capture-overlay__layer-controls" aria-hidden="true" />

    <template v-if="selectionCss">
      <div v-for="(style, side) in dimmerStyles" :key="side" class="screen-capture-overlay__dimmer" :data-dimmer="side" :style="style" />
      <div class="screen-capture-overlay__selection" data-testid="selection-chrome" :style="selectionStyle">
        <button
          v-for="handle in selectionHandles"
          :key="handle.name"
          type="button"
          class="screen-capture-overlay__handle"
          :class="`screen-capture-overlay__handle--${handle.name}`"
          :data-handle="handle.name"
          :aria-label="resizeSelectionLabel(handle.name)"
          :title="resizeSelectionLabel(handle.name)"
          :style="handle.style"
        />
        <output class="screen-capture-overlay__dimensions" data-testid="selection-dimensions" aria-live="polite">{{ selectionDimensions }}</output>
      </div>

      <div ref="toolbarHostRef" class="screen-capture-overlay__toolbar" :style="toolbarStyle">
        <ScreenCaptureToolbar
          :active-tool="activeCaptureTool"
          :color="annotation.tools.activeColor.value"
          :stroke-width="annotation.tools.activeLineWidth.value"
          :can-undo="annotation.history.canUndo.value"
          :can-redo="annotation.history.canRedo.value"
          :busy="controlsLocked"
          :can-confirm="canConfirm"
          :messages="messages"
          @tool="selectTool"
          @color="selectColor"
          @stroke-width="selectStrokeWidth"
          @undo="undo"
          @redo="redo"
          @action="performAction"
          @cancel="cancelCapture"
        />
      </div>
    </template>

    <textarea
      v-if="annotation.text.draft.value"
      ref="textInputRef"
      class="screen-capture-overlay__text-input"
      :aria-label="messages.a11y.annotationText"
      :style="textInputStyle"
      :value="annotation.text.draft.value.text"
      @pointerdown.stop
      @dblclick.stop
      @contextmenu.stop="onContextMenu"
      @input="updateText"
      @keydown="onTextKeyDown"
    />

    <div v-if="busyStatus" class="screen-capture-overlay__status" role="status" aria-live="polite">
      {{ busyStatus }}
    </div>

    <div v-if="errorMessage" class="screen-capture-overlay__error" role="alert">
      {{ errorMessage }}
    </div>
  </div>
</template>

<script lang="ts">
import type { SelectionRect as CaptureSelectionRect } from './selection-model'
import type { CaptureExportAction as ToolbarExportAction } from './ScreenCaptureToolbar.vue'

export interface CaptureExportPayload {
  png: Blob
  selection: CaptureSelectionRect
}

export interface ScreenCaptureOverlayHandle {
  awaitPaint(): Promise<void>
  dispose(): void
}

export type ScreenCaptureExportCallback = (action: ToolbarExportAction, payload: CaptureExportPayload) => Promise<void>
</script>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, type CSSProperties } from 'vue'
import { useAnnotationSession } from '@/views/image-editor/composables/useAnnotationSession'
import { cssPointToFrame, frameRectToCss, selectionToExportRect, type CssViewportRect } from './capture-geometry'
import { defaultCaptureMessages, formatCaptureMessage, type CaptureMessages } from './capture-localization'
import { CaptureSelection, captureToolbarWorkArea, handleCenters, placeCaptureToolbar, type FrameBounds, type FramePoint, type SelectionHandle, type SelectionRect } from './selection-model'
import ScreenCaptureToolbar, { type CaptureAnnotationTool, type CaptureExportAction } from './ScreenCaptureToolbar.vue'

interface Props {
  frame: ImageData | null
  onExport: (action: CaptureExportAction, payload: CaptureExportPayload) => Promise<void>
  onCancel: () => Promise<void> | void
  canConfirm?: boolean
  messages?: CaptureMessages
  visibleArea?: SelectionRect | null
}

type DesktopScreen = Screen & {
  availLeft?: number
  availTop?: number
}

const props = withDefaults(defineProps<Props>(), {
  canConfirm: true,
  messages: () => defaultCaptureMessages,
  visibleArea: null,
})
const emit = defineEmits<{ frameInstalled: [] }>()

let initialFrame = props.frame
if (!initialFrame) throw new Error('screen capture frame is required during overlay mount')

const annotation = useAnnotationSession()
const { canvasRef, overlayRef, wrapRef } = annotation
const stageRef = ref<HTMLElement | null>(null)
const frozenCanvasRef = ref<HTMLCanvasElement | null>(null)
const toolbarHostRef = ref<HTMLElement | null>(null)
const textInputRef = ref<HTMLTextAreaElement | null>(null)
const frameBounds: FrameBounds = { width: initialFrame.width, height: initialFrame.height }
let selection = new CaptureSelection(frameBounds)
const selectionRect = ref<SelectionRect | null>(null)
const activeCaptureTool = ref<CaptureAnnotationTool | null>(null)
const activePointer = ref<{ pointerId: number; owner: 'selection' | 'annotation'; captureTarget: HTMLElement } | null>(null)
const busyAction = ref<CaptureExportAction | 'cancel' | null>(null)
const errorMessage = ref('')
const viewportRevision = ref(0)
const toolbarPoint = ref<FramePoint>({ x: 0, y: 0 })
let disposed = false
let actionGeneration = 0
let resizeObserver: ResizeObserver | null = null

const controlsLocked = computed(() => busyAction.value !== null || activePointer.value !== null || annotation.text.draft.value !== null)
const busyStatus = computed(() => {
  switch (busyAction.value) {
    case 'save':
      return props.messages.status.saving
    case 'copy':
      return props.messages.status.copying
    case 'confirm':
      return props.messages.status.sending
    case 'cancel':
      return props.messages.status.cancelling
    default:
      return ''
  }
})
const selectionDimensions = computed(() => {
  const rect = selectionRect.value
  return rect ? `${rect.width} × ${rect.height}` : ''
})

function viewportRect(): CssViewportRect {
  viewportRevision.value
  const rect = stageRef.value?.getBoundingClientRect()
  if (rect && rect.width > 0 && rect.height > 0) return { left: rect.left, top: rect.top, width: rect.width, height: rect.height }
  return { left: 0, top: 0, width: frameBounds.width, height: frameBounds.height }
}

const selectionCss = computed<SelectionRect | null>(() => {
  const rect = selectionRect.value
  if (!rect) return null
  const viewport = viewportRect()
  const mapped = frameRectToCss(rect, viewport, frameBounds)
  return { x: mapped.x - viewport.left, y: mapped.y - viewport.top, width: mapped.width, height: mapped.height }
})

const selectionStyle = computed<CSSProperties>(() => {
  const rect = selectionCss.value
  if (!rect) return {}
  return { left: `${rect.x}px`, top: `${rect.y}px`, width: `${rect.width}px`, height: `${rect.height}px` }
})

const dimmerStyles = computed<Record<'top' | 'right' | 'bottom' | 'left', CSSProperties>>(() => {
  const rect = selectionCss.value
  const viewport = viewportRect()
  if (!rect) return { top: {}, right: {}, bottom: {}, left: {} }
  const right = rect.x + rect.width
  const bottom = rect.y + rect.height
  return {
    top: { left: '0', top: '0', width: '100%', height: `${Math.max(0, rect.y)}px` },
    right: { left: `${right}px`, top: `${rect.y}px`, right: '0', height: `${rect.height}px` },
    bottom: { left: '0', top: `${bottom}px`, width: '100%', height: `${Math.max(0, viewport.height - bottom)}px` },
    left: { left: '0', top: `${rect.y}px`, width: `${Math.max(0, rect.x)}px`, height: `${rect.height}px` },
  }
})

const selectionHandles = computed<Array<{ name: SelectionHandle; style: CSSProperties }>>(() => {
  const frameRect = selectionRect.value
  const cssRect = selectionCss.value
  if (!frameRect || !cssRect) return []
  const centers = handleCenters(frameRect)
  return (Object.keys(centers) as SelectionHandle[]).map((name) => {
    const center = centers[name]
    const x = ((center.x - frameRect.x) * cssRect.width) / frameRect.width
    const y = ((center.y - frameRect.y) * cssRect.height) / frameRect.height
    return { name, style: { left: `${x}px`, top: `${y}px` } }
  })
})

const toolbarStyle = computed<CSSProperties>(() => ({ left: `${toolbarPoint.value.x}px`, top: `${toolbarPoint.value.y}px` }))

const textInputStyle = computed<CSSProperties>(() => {
  const draft = annotation.text.draft.value
  if (!draft) return {}
  const viewport = viewportRect()
  const mapped = frameRectToCss({ x: draft.x, y: draft.y, width: Math.max(1, draft.maxWidth), height: Math.max(1, draft.fontSize * 1.35) }, viewport, frameBounds)
  return {
    left: `${mapped.x - viewport.left}px`,
    top: `${mapped.y - viewport.top}px`,
    width: `${Math.max(120, mapped.width)}px`,
    minHeight: `${Math.max(34, mapped.height)}px`,
    color: draft.color,
    fontSize: `${Math.max(14, (draft.fontSize * viewport.height) / frameBounds.height)}px`,
  }
})

function pointFromEvent(event: MouseEvent): FramePoint {
  return cssPointToFrame({ x: event.clientX, y: event.clientY }, viewportRect(), frameBounds)
}

function isInside(rect: SelectionRect, point: FramePoint): boolean {
  return point.x >= rect.x && point.x <= rect.x + rect.width && point.y >= rect.y && point.y <= rect.y + rect.height
}

function syncSelection() {
  selectionRect.value = selection.rect ? { ...selection.rect } : null
  void nextTick(updateToolbarPosition)
}

function updateToolbarPosition() {
  const rect = selectionCss.value
  if (!rect) return
  const viewport = viewportRect()
  const toolbarRect = toolbarHostRef.value?.getBoundingClientRect()
  const toolbar = {
    width: toolbarRect && toolbarRect.width > 0 ? toolbarRect.width : Math.min(620, viewport.width),
    height: toolbarRect && toolbarRect.height > 0 ? toolbarRect.height : 56,
  }
  const viewportScreenOrigin = { x: window.screenX + viewport.left, y: window.screenY + viewport.top }
  const desktopScreen = window.screen as DesktopScreen
  const visibleArea =
    props.visibleArea ??
    captureToolbarWorkArea({ width: viewport.width, height: viewport.height }, viewportScreenOrigin, {
      x: desktopScreen.availLeft ?? viewportScreenOrigin.x,
      y: desktopScreen.availTop ?? viewportScreenOrigin.y,
      width: desktopScreen.availWidth,
      height: desktopScreen.availHeight,
    })
  toolbarPoint.value = placeCaptureToolbar(rect, toolbar, { width: viewport.width, height: viewport.height }, 8, visibleArea)
}

function capturePointer(target: HTMLElement, pointerId: number) {
  try {
    target.setPointerCapture(pointerId)
  } catch {
    // Synthetic events and a pointer already cancelled by the host may not be capturable.
  }
}

function releasePointer(pointer: { pointerId: number; captureTarget: HTMLElement }) {
  try {
    if (pointer.captureTarget.hasPointerCapture(pointer.pointerId)) pointer.captureTarget.releasePointerCapture(pointer.pointerId)
  } catch {
    // The browser may have released capture before dispatching pointercancel.
  }
}

function selectionHandle(target: EventTarget | null): SelectionHandle | undefined {
  if (!(target instanceof Element)) return undefined
  const value = target.closest<HTMLElement>('[data-handle]')?.dataset.handle
  return value && ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'].includes(value) ? (value as SelectionHandle) : undefined
}

function onPointerDown(event: PointerEvent) {
  if (disposed || event.button !== 0 || activePointer.value) return
  const target = event.currentTarget
  if (!(target instanceof HTMLElement)) return
  const point = pointFromEvent(event)
  const handle = selectionHandle(event.target)
  const rect = selection.rect
  const selectionGesture = !rect || Boolean(handle) || !isInside(rect, point) || activeCaptureTool.value === null

  event.preventDefault()
  capturePointer(target, event.pointerId)
  if (selectionGesture) {
    selection.begin(event.pointerId, point, handle)
    activePointer.value = { pointerId: event.pointerId, owner: 'selection', captureTarget: target }
    syncSelection()
    return
  }

  const tool = activeCaptureTool.value
  if (!tool) return
  annotation.tools.activeTool.value = tool
  annotation.pointer.onPointerDown(event)
  activePointer.value = { pointerId: event.pointerId, owner: 'annotation', captureTarget: target }
  if (activeCaptureTool.value === 'text') void nextTick(() => textInputRef.value?.focus())
}

function onPointerMove(event: PointerEvent) {
  const pointer = activePointer.value
  if (!pointer || pointer.pointerId !== event.pointerId) return
  event.preventDefault()
  if (pointer.owner === 'selection') {
    selection.update(event.pointerId, pointFromEvent(event))
    syncSelection()
  } else {
    annotation.pointer.onPointerMove(event)
  }
}

function onPointerUp(event: PointerEvent) {
  const pointer = activePointer.value
  if (!pointer || pointer.pointerId !== event.pointerId) return
  event.preventDefault()
  if (pointer.owner === 'selection') {
    selection.end(event.pointerId)
    syncSelection()
  } else {
    annotation.pointer.onPointerUp()
  }
  releasePointer(pointer)
  activePointer.value = null
}

function cancelActivePointer() {
  const pointer = activePointer.value
  if (!pointer) return false
  if (pointer.owner === 'selection') {
    selection.cancelInteraction(pointer.pointerId)
    syncSelection()
  } else {
    annotation.pointer.onPointerCancel()
  }
  releasePointer(pointer)
  activePointer.value = null
  return true
}

function onPointerCancel(event: PointerEvent) {
  const pointer = activePointer.value
  if (!pointer || pointer.pointerId !== event.pointerId) return
  event.preventDefault()
  cancelActivePointer()
}

function selectTool(tool: CaptureAnnotationTool) {
  if (controlsLocked.value) return
  activeCaptureTool.value = activeCaptureTool.value === tool ? null : tool
  annotation.tools.activeTool.value = activeCaptureTool.value ?? 'select'
}

function selectColor(color: string) {
  if (!controlsLocked.value) annotation.tools.activeColor.value = color
}

function selectStrokeWidth(width: number) {
  if (!controlsLocked.value) annotation.tools.activeLineWidth.value = width
}

function undo() {
  if (!controlsLocked.value) annotation.history.undo()
}

function redo() {
  if (!controlsLocked.value) annotation.history.redo()
}

function cancelText() {
  if (!annotation.text.draft.value) return false
  annotation.text.cancel()
  activeCaptureTool.value = null
  return true
}

function updateText(event: Event) {
  annotation.text.update((event.target as HTMLTextAreaElement).value)
}

function commitText() {
  const input = textInputRef.value
  annotation.text.commit(input?.value)
  activeCaptureTool.value = null
}

function onTextKeyDown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    cancelText()
  } else if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault()
    event.stopPropagation()
    commitText()
  }
}

function actionError(action: CaptureExportAction): string {
  if (action === 'save') return props.messages.errors.saveFailed
  if (action === 'copy') return props.messages.errors.copyFailed
  return props.messages.errors.sendFailed
}

function resizeSelectionLabel(handle: SelectionHandle): string {
  return formatCaptureMessage(props.messages.a11y.resizeSelection, { handle })
}

async function performAction(action: CaptureExportAction) {
  if (disposed || busyAction.value || activePointer.value || annotation.text.draft.value) return
  if (action === 'confirm' && !props.canConfirm) return
  const rect = selection.rect
  if (!rect) return
  const generation = ++actionGeneration
  busyAction.value = action
  errorMessage.value = ''
  try {
    const exportRect = selectionToExportRect(rect, frameBounds)
    const rendered = await annotation.exportOps.renderPng(exportRect)
    if (!rendered.ok) throw new Error(`capture_export_failed:${rendered.error}`)
    await props.onExport(action, { png: rendered.value, selection: exportRect })
    if (!disposed && generation === actionGeneration) errorMessage.value = ''
  } catch (error) {
    console.warn('screen capture action failed', error)
    if (!disposed && generation === actionGeneration) errorMessage.value = actionError(action)
  } finally {
    if (!disposed && generation === actionGeneration) busyAction.value = null
  }
}

async function cancelCapture() {
  if (disposed || busyAction.value) return
  const generation = ++actionGeneration
  busyAction.value = 'cancel'
  errorMessage.value = ''
  try {
    await props.onCancel()
  } catch (error) {
    console.warn('screen capture cancellation failed', error)
    if (!disposed && generation === actionGeneration) errorMessage.value = props.messages.errors.cancelFailed
  } finally {
    if (!disposed && generation === actionGeneration) busyAction.value = null
  }
}

function isEditableTarget(target: EventTarget | null): boolean {
  return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || (target instanceof HTMLElement && target.isContentEditable)
}

function onWindowKeyDown(event: KeyboardEvent) {
  if (disposed || event.defaultPrevented || isEditableTarget(event.target)) return
  if (event.key === 'Escape') {
    event.preventDefault()
    if (cancelText()) return
    if (cancelActivePointer()) return
    void cancelCapture()
    return
  }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'z') {
    event.preventDefault()
    if (activePointer.value || annotation.text.draft.value || busyAction.value) return
    if (event.shiftKey) annotation.history.redo()
    else annotation.history.undo()
    return
  }
  if (event.key === 'Enter' && !event.shiftKey && !event.ctrlKey && !event.metaKey && !event.altKey) {
    event.preventDefault()
    void performAction('confirm')
  }
}

function onContextMenu(event: MouseEvent) {
  event.preventDefault()
  if (cancelText()) return
  if (cancelActivePointer()) return
  void cancelCapture()
}

function onDoubleClick(event: MouseEvent) {
  event.preventDefault()
  if (activePointer.value || annotation.text.draft.value || busyAction.value || !selection.rect) return
  if (isInside(selection.rect, pointFromEvent(event))) void performAction('confirm')
}

async function awaitPaint(): Promise<void> {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    if (disposed) throw new Error('ScreenCaptureOverlay is disposed')
    await nextTick()
    const result = await annotation.render.awaitPaint()
    if (result.ok) return
    if (result.error === 'disposed') throw new Error('ScreenCaptureOverlay is disposed')
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
  }
  throw new Error('Screen capture preview did not become ready')
}

function installFrame() {
  const frame = initialFrame
  if (!frame) throw new Error('screen capture frame was already released')
  const source = frozenCanvasRef.value
  if (!source) throw new Error('screen capture source canvas is unavailable')
  source.width = frameBounds.width
  source.height = frameBounds.height
  const context = source.getContext('2d', { alpha: false })
  if (!context) throw new Error('screen capture source context is unavailable')
  context.putImageData(frame, 0, 0)
  initialFrame = null
  emit('frameInstalled')
  canvasRef.value?.setAttribute('width', String(frameBounds.width))
  canvasRef.value?.setAttribute('height', String(frameBounds.height))
  overlayRef.value?.setAttribute('width', String(frameBounds.width))
  overlayRef.value?.setAttribute('height', String(frameBounds.height))
  annotation.source.set({
    drawable: source,
    width: frameBounds.width,
    height: frameBounds.height,
    release() {
      context.clearRect(0, 0, source.width, source.height)
      source.width = 1
      source.height = 1
    },
  })
}

function dispose() {
  if (disposed) return
  disposed = true
  actionGeneration += 1
  cancelActivePointer()
  cancelText()
  resizeObserver?.disconnect()
  resizeObserver = null
  window.removeEventListener('keydown', onWindowKeyDown)
  annotation.dispose()
}

watch(selectionCss, () => void nextTick(updateToolbarPosition))

onMounted(() => {
  wrapRef.value = stageRef.value
  installFrame()
  window.addEventListener('keydown', onWindowKeyDown)
  if (stageRef.value && typeof ResizeObserver !== 'undefined') {
    resizeObserver = new ResizeObserver(() => {
      viewportRevision.value += 1
      updateToolbarPosition()
    })
    resizeObserver.observe(stageRef.value)
  }
})

onBeforeUnmount(dispose)

defineExpose<ScreenCaptureOverlayHandle>({ awaitPaint, dispose })
</script>

<style scoped>
.screen-capture-overlay {
  position: fixed;
  inset: 0;
  overflow: hidden;
  color: #f8fafc;
  background: #000;
  cursor: crosshair;
  touch-action: none;
  user-select: none;
}

.screen-capture-overlay__canvas {
  position: absolute;
  inset: 0;
  display: block;
  width: 100%;
  height: 100%;
}

.screen-capture-overlay__source {
  z-index: 0;
}

.screen-capture-overlay__annotations {
  z-index: 1;
}

.screen-capture-overlay__layer-controls {
  z-index: 2;
  pointer-events: none;
}

.screen-capture-overlay__dimmer {
  position: absolute;
  z-index: 3;
  background: rgb(0 0 0 / 52%);
  pointer-events: none;
}

.screen-capture-overlay__selection {
  position: absolute;
  z-index: 4;
  box-sizing: border-box;
  border: 1px solid #60a5fa;
  pointer-events: none;
}

.screen-capture-overlay__handle {
  position: absolute;
  width: 10px;
  height: 10px;
  padding: 0;
  background: #f8fafc;
  border: 1px solid #2563eb;
  border-radius: 50%;
  transform: translate(-50%, -50%);
  pointer-events: auto;
}

.screen-capture-overlay__handle--nw,
.screen-capture-overlay__handle--se {
  cursor: nwse-resize;
}

.screen-capture-overlay__handle--ne,
.screen-capture-overlay__handle--sw {
  cursor: nesw-resize;
}

.screen-capture-overlay__handle--n,
.screen-capture-overlay__handle--s {
  cursor: ns-resize;
}

.screen-capture-overlay__handle--e,
.screen-capture-overlay__handle--w {
  cursor: ew-resize;
}

.screen-capture-overlay__dimensions {
  position: absolute;
  left: 0;
  top: -28px;
  padding: 3px 7px;
  color: #fff;
  font:
    500 12px/1.4 system-ui,
    sans-serif;
  white-space: nowrap;
  background: rgb(17 24 39 / 88%);
  border-radius: 4px;
}

.screen-capture-overlay__toolbar {
  position: absolute;
  z-index: 6;
  cursor: default;
}

.screen-capture-overlay__text-input {
  position: absolute;
  z-index: 7;
  box-sizing: border-box;
  padding: 3px 5px;
  overflow: hidden;
  font-family: sans-serif;
  line-height: 1.25;
  resize: both;
  background: rgb(15 23 42 / 72%);
  border: 1px dashed #93c5fd;
  border-radius: 3px;
  outline: none;
}

.screen-capture-overlay__status,
.screen-capture-overlay__error {
  position: absolute;
  right: 16px;
  bottom: 16px;
  z-index: 8;
  max-width: min(460px, calc(100% - 32px));
  padding: 9px 12px;
  color: #fee2e2;
  font:
    500 13px/1.4 system-ui,
    sans-serif;
  border-radius: 6px;
}

.screen-capture-overlay__status {
  color: #e2e8f0;
  background: rgb(17 24 39 / 94%);
  border: 1px solid #64748b;
}

.screen-capture-overlay__error {
  background: rgb(127 29 29 / 94%);
  border: 1px solid #f87171;
}
</style>
