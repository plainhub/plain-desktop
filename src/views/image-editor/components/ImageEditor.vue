<template>
  <div
    class="editor-card"
    :class="{ fullscreen: isFullscreen }"
  >
    <div class="toolbar-bar">
        <ImageEditorToolbar
          :active-tool="activeTool"
          :active-color="activeColor"
          :active-line-width="activeLineWidth"
          :can-undo="canUndo"
          :can-redo="canRedo"
          :is-cropping="isCropping"
          @update:active-tool="activeTool = $event"
          @update:active-color="activeColor = $event"
          @update:active-line-width="activeLineWidth = $event"
          @undo="undo"
          @redo="redo"
          @add-image-file="addImageLayerFromFile"
        />
        <HeaderActions :logged-in="isLoggedIn" class="header-actions" />
      </div>

      <div v-if="isCropping" class="crop-bar">
        <i-lucide-crop class="crop-icon" />
        <span class="crop-hint">{{ $t('image_editor.crop_hint') }}</span>
        <span v-if="cropRect && cropRect.w > 1 && cropRect.h > 1" class="crop-dims">
          {{ Math.round(cropRect.w) }} × {{ Math.round(cropRect.h) }}px
        </span>
        <div class="crop-spacer" />
        <button v-if="cropRect && cropRect.w > 1 && cropRect.h > 1" class="crop-btn apply" @click="applyCrop">
          {{ $t('image_editor.apply_crop') }}
        </button>
        <button class="crop-btn cancel" @click="cancelCrop">
          {{ $t('image_editor.cancel') }}
        </button>
      </div>

      <div class="editor-grid" :class="{ 'flex-fill': isFullscreen }">
        <div
          class="canvas-area"
          :class="{ 'fullscreen-pad': isFullscreen }"
          @wheel.prevent="onCanvasWheel"
        >
          <div
            ref="wrapRef"
            class="canvas-wrap"
            :style="canvasWrapStyle"
          >
            <canvas
              ref="canvasRef"
              class="main-canvas"
            />
            <textarea
              v-if="inlineEditLayerId && inlineEditLayer"
              ref="inlineEditRef"
              :value="inlineEditLayer.text"
              :class="inlineEditLayer.type === 'sticker' ? 'inline-edit-sticker' : 'inline-edit-text'"
              :style="inlineEditPositionStyle"
              spellcheck="false"
              @input="handleInlineInput"
              @blur="commitInlineEdit"
              @keydown.enter.exact="onInlineEnter"
              @keydown.escape.prevent="commitInlineEdit"
            />
            <canvas
              ref="overlayRef"
              :width="canvasRenderWidth"
              :height="canvasRenderHeight"
              class="overlay-canvas"
              :style="{ cursor: canvasCursor }"
              @pointerdown="handleOverlayPointerDown"
              @pointermove="handleOverlayPointerMove"
              @pointerup="handleOverlayPointerUp"
              @pointercancel="handleOverlayPointerUp"
              @dblclick="handleDblClick"
            />
          </div>

          <div
            v-if="editorActive && canvasZoom !== 1"
            class="zoom-indicator"
          >
            <span class="zoom-label" @click="resetCanvasZoom">
              {{ Math.round(canvasZoom * 100) }}%
            </span>
          </div>
        </div>

        <div class="sidebar" :class="{ 'fullscreen-pad': isFullscreen }">
          <div :class="{ 'cropping-dim': isCropping }">
            <ImageEditorSidebar
              :canvas-width="canvasSize.width"
              :canvas-height="canvasSize.height"
              :layers="layers"
              :selected-layer-id="selectedLayerId"
              @resize="resizeCanvas"
              @select-layer="selectedLayerId = $event"
              @remove-layer="removeLayer"
              @reorder-layer="reorderLayer"
              @toggle-visibility="toggleLayerVisibility"
              @clear-all="clearLayers"
              @replace-image-file="replaceImageLayerFile"
              @open-text-editor="openTextEditor"
            />
          </div>

          <hr class="divider" />

          <ImageEditorBackground
            v-model:bg-mode="bgMode"
            v-model:bg-color="bgColor"
            v-model:bg-image-alpha="bgImageAlpha"
            :photo-src="bgPhotoSrc"
            :bg-offset="bgOffset"
            @photo-select="onBgPhotoSelect"
            @remove-photo="removeBgPhoto"
            @reset-bg-offset="bgOffset = { x: 0.5, y: 0.5 }"
          />

          <hr class="divider" />

          <div class="actions-section">
            <button class="action-btn outline" @click="openPreview">
              <i-lucide-eye />
              {{ $t('image_editor.preview_image') }}
            </button>

            <SaveFormatButton @save="download" @copy="copyToClipboard" />

            <button
              id="editor-delete-project-btn"
              class="action-btn danger"
              :disabled="deleteLoading"
              @click="showDeleteConfirm = true"
            >
              <i-lucide-trash-2 />
              {{ $t('image_editor.delete_project') }}
            </button>
            <v-dropdown-menu v-model="showDeleteConfirm" anchor="editor-delete-project-btn">
              <inline-delete-confirm
                :loading="deleteLoading"
                message-key="image_editor.delete_project_confirm"
                @confirm="handleDeleteProject"
                @cancel="showDeleteConfirm = false"
              />
            </v-dropdown-menu>
          </div>
        </div>
      </div>
  </div>

  <TextLayerEditor
    :layer="popupLayer"
    :rect="popupRect"
    :doc="doc"
    @close="popupLayerId = null"
  />

  <ImageEditorStickerPopup
    :layer="stickerPopupLayer"
    :rect="stickerPopupRect"
    :doc="doc"
    @close="stickerPopupId = null"
  />

  <ImageLightbox v-model="showPreview" :images="previewImages" />
</template>

<script setup lang="ts">
import { ref, computed, watch, onUnmounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { useImageEditorCore } from '@/views/image-editor/composables/useImageEditorCore'
import { useImageEditorInlineEdit } from '@/views/image-editor/composables/useImageEditorInlineEdit'
import { useEditorPopups } from '@/views/image-editor/composables/useEditorPopups'
import { useCanvasZoom } from '@/views/image-editor/composables/useCanvasZoom'
import { useOverlayPointer } from '@/views/image-editor/composables/useOverlayPointer'
import { useImageEditorKeyboard } from '@/views/image-editor/composables/useImageEditorKeyboard'
import { getCurrentAuthToken } from '@/lib/device/current'
import HeaderActions from '@/components/HeaderActions.vue'

const { t } = useI18n()

const isLoggedIn = computed(() => !!getCurrentAuthToken())

const {
  canvasRef, overlayRef, wrapRef, doc,
  state, tools, crop: cropApi, history, render: renderApi,
  image: imageApi, pointer, layerOps, sticker: stickerApi, exportOps, persistence,
} = useImageEditorCore()

const {
  sourceImg, canvasSize, bgColor, imgAlpha, editorActive,
  isFullscreen, inlineEditLayerId, renderScale,
  layers, selectedLayerId,
} = state
const { activeTool, activeColor, activeLineWidth, overlayCursor } = tools
const { isCropping, cropRect, applyCrop, cancelCrop } = cropApi
const { undo, redo, canUndo, canRedo, pushUndo } = history
const { draw, resizeCanvas } = renderApi
const { loadImage, loadImageFromUrl, startBlank } = imageApi
const { onPointerDown, onPointerMove, onPointerUp, onDoubleClick } = pointer
const {
  clearLayers, removeLayer, reorderLayer, toggleLayerVisibility,
  addTextLayer, addStickerLayer, addImageLayerFromFile, replaceImageLayerFile,
} = layerOps
const { autoResizeSticker } = stickerApi
const { download, copyToClipboard, getPreviewBlobUrl } = exportOps
const { deleteProject } = persistence

const showPreview = ref(false)
const previewImages = ref<{ src: string; label: string }[]>([])
let previewBlobUrl: string | null = null

async function openPreview() {
  previewBlobUrl = await getPreviewBlobUrl()
  previewImages.value = [{ src: previewBlobUrl, label: t('image_editor.preview_image') }]
  showPreview.value = true
}

watch(showPreview, (open) => {
  if (!open) {
    if (previewBlobUrl) {
      URL.revokeObjectURL(previewBlobUrl)
      previewBlobUrl = null
    }
    previewImages.value = []
    draw()
  }
})

onUnmounted(() => {
  if (previewBlobUrl) URL.revokeObjectURL(previewBlobUrl)
})

const showDeleteConfirm = ref(false)
const deleteLoading = ref(false)

async function handleDeleteProject() {
  if (deleteLoading.value) return
  deleteLoading.value = true
  try {
    await deleteProject()
  } finally {
    deleteLoading.value = false
  }
}

const bgMode = ref<'color' | 'image'>(sourceImg.value ? 'image' : 'color')
const bgImageAlpha = imgAlpha
const bgPhotoSrc = computed(() => sourceImg.value instanceof HTMLImageElement ? sourceImg.value.src : null)
const bgOffset = ref<{ x: number; y: number }>({ x: 0.5, y: 0.5 })

watch(sourceImg, (img) => {
  bgMode.value = img ? 'image' : 'color'
})

async function onBgPhotoSelect(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  ;(e.target as HTMLInputElement).value = ''
  await loadImage(file)
}

function removeBgPhoto() {
  sourceImg.value = null
  bgMode.value = 'color'
}

watch(activeColor, (c) => {
  const sel = layers.find(l => l.id === selectedLayerId.value)
  if (sel && 'color' in sel && sel.type !== 'text' && sel.type !== 'sticker') {
    pushUndo()
    doc.setLayerProp(sel.id, 'color', c)
  }
})
watch(activeLineWidth, (lw) => {
  const sel = layers.find(l => l.id === selectedLayerId.value)
  if (sel && 'lineWidth' in sel) {
    pushUndo()
    doc.setLayerProp(sel.id, 'lineWidth', lw)
  }
})

const {
  inlineEditLayer, inlineEditRef, inlineEditPositionStyle,
  startInlineEdit, commitInlineEdit, onInlineEditInput,
} = useImageEditorInlineEdit(
  layers, wrapRef, canvasSize, selectedLayerId, inlineEditLayerId, doc,
  { draw },
)

function onInlineEnter(e: KeyboardEvent) {
  const layer = inlineEditLayer.value
  if (layer && layer.type === 'sticker') {
    return
  }
  e.preventDefault()
  commitInlineEdit()
}

function handleInlineInput(e: Event) {
  onInlineEditInput(e)
  const layer = inlineEditLayer.value
  if (layer && layer.type === 'sticker') {
    autoResizeSticker(layer as any)
    const wrap = wrapRef.value
    if (wrap) {
      const scale = wrap.clientWidth / canvasSize.value.width
      const hDisplay = (layer as any).h * scale
      inlineEditPositionStyle.value = {
        ...inlineEditPositionStyle.value,
        'min-height': `${hDisplay}px`,
      }
    }
  }
}

useImageEditorKeyboard(layers, selectedLayerId, inlineEditLayerId, doc, {
  removeLayer, addTextLayer, addImageLayerFromFile, pushUndo, draw,
})

const {
  popupLayerId, popupRect, popupLayer,
  stickerPopupId, stickerPopupRect, stickerPopupLayer,
  openTextEditor,
} = useEditorPopups(layers, selectedLayerId, wrapRef, draw)

const {
  zoom: canvasZoom, panX: canvasPanX, panY: canvasPanY,
  isSpaceDown,
  onWheel: onCanvasWheel, resetZoom: resetCanvasZoom,
  startPan, movePan, endPan,
} = useCanvasZoom(wrapRef, editorActive, canvasSize)

const dpr = window.devicePixelRatio || 1
const fitScale = ref(1)

function updateFitScale() {
  const el = wrapRef.value
  if (el && el.clientWidth > 0 && canvasSize.value.width > 0) {
    fitScale.value = el.clientWidth / canvasSize.value.width
  }
}

let resizeObserver: ResizeObserver | null = null

watch(wrapRef, (el) => {
  resizeObserver?.disconnect()
  resizeObserver = null
  if (el) {
    updateFitScale()
    resizeObserver = new ResizeObserver(updateFitScale)
    resizeObserver.observe(el)
  }
}, { immediate: true })

onUnmounted(() => { resizeObserver?.disconnect() })

watch(canvasSize, () => nextTick(updateFitScale))

const canvasRenderWidth = computed(() => Math.max(1, Math.round(canvasSize.value.width * renderScale.value)))
const canvasRenderHeight = computed(() => Math.max(1, Math.round(canvasSize.value.height * renderScale.value)))

watch([fitScale, canvasZoom], () => {
  renderScale.value = fitScale.value * canvasZoom.value * dpr
}, { immediate: true })

const canvasWrapStyle = computed(() => {
  const base: Record<string, string> = {
    aspectRatio: `${canvasSize.value.width} / ${canvasSize.value.height}`,
  }
  if (canvasZoom.value !== 1 || canvasPanX.value || canvasPanY.value) {
    base.transform = `translate(${canvasPanX.value}px, ${canvasPanY.value}px) scale(${canvasZoom.value})`
  }
  if (isFullscreen.value) {
    return {
      ...base,
      maxHeight: '100%',
      maxWidth: `min(100%, calc(100% * ${canvasSize.value.width} / ${canvasSize.value.height}))`,
      flex: '1 1 0%',
    }
  }
  return {
    ...base,
    maxHeight: '70vh',
    maxWidth: `min(100%, calc(70vh * ${canvasSize.value.width} / ${canvasSize.value.height}))`,
  }
})

const canvasCursor = computed(() => {
  if (isSpaceDown.value) return 'grab'
  if (activeTool.value === 'crop') return overlayCursor.value || 'crosshair'
  if (activeTool.value === 'select') return overlayCursor.value
  if (activeTool.value === 'text') return 'text'
  if (activeTool.value === 'sticker') return 'crosshair'
  if (activeTool.value === 'brush') return 'crosshair'
  return 'crosshair'
})

function handleDblClick(e: MouseEvent) {
  const layerId = onDoubleClick(e)
  if (layerId) startInlineEdit(layerId)
}

const { handleOverlayPointerDown, handleOverlayPointerMove, handleOverlayPointerUp } = useOverlayPointer(
  isSpaceDown,
  { startPan, movePan, endPan },
  { down: onPointerDown, move: onPointerMove, up: onPointerUp },
)

defineExpose({ loadImage, loadImageFromUrl, startBlank })
</script>

<style lang="scss" scoped>
.editor-card {
  overflow: hidden;
  background: var(--md-sys-color-surface-container-low);
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 16px;
  box-shadow: 0 10px 15px -3px rgba(0,0,0,0.05);
  transition: all 0.3s;

  &.fullscreen {
    position: fixed;
    inset: 0;
    z-index: 100;
    border: none;
    border-radius: 0;
    box-shadow: none;
    display: flex;
    flex-direction: column;
  }
}

.toolbar-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--md-sys-color-outline-variant);
  background: color-mix(in srgb, var(--md-sys-color-surface-container-high) 80%, transparent);
  flex-shrink: 0;
}

.header-actions {
  margin-left: auto;
}

.crop-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 16px;
  border-bottom: 1px solid color-mix(in srgb, var(--md-sys-color-primary) 20%, transparent);
  background: color-mix(in srgb, var(--md-sys-color-primary) 10%, transparent);
  flex-shrink: 0;
}

.crop-icon { font-size: 16px; color: var(--md-sys-color-primary); }

.crop-hint { font-size: 14px; color: var(--md-sys-color-primary); font-weight: 500; }

.crop-dims {
  font-size: 12px;
  color: color-mix(in srgb, var(--md-sys-color-primary) 70%, transparent);
  font-family: monospace;
  font-variant-numeric: tabular-nums;
}

.crop-spacer { flex: 1; }

.crop-btn {
  padding: 6px 12px;
  font-size: 14px;
  font-weight: 500;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  font-family: inherit;
  transition: all 0.15s;

  &.apply {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    &:hover { filter: brightness(1.05); }
  }
  &.cancel {
    background: transparent;
    color: var(--md-sys-color-on-surface-variant);
    &:hover { background: color-mix(in srgb, var(--md-sys-color-on-surface) 5%, transparent); }
  }
}

.editor-grid {
  display: grid;
  grid-template-columns: 1fr;

  &.flex-fill { flex: 1; min-height: 0; }
}

@media (min-width: 1024px) {
  .editor-grid { grid-template-columns: 1fr 280px; }
}

.canvas-area {
  min-width: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  position: relative;
  padding: 16px;
  background-color: var(--md-sys-color-surface-container-low);
  background-image:
    linear-gradient(45deg, color-mix(in srgb, var(--md-sys-color-outline) 15%, transparent) 25%, transparent 25%),
    linear-gradient(-45deg, color-mix(in srgb, var(--md-sys-color-outline) 15%, transparent) 25%, transparent 25%),
    linear-gradient(45deg, transparent 75%, color-mix(in srgb, var(--md-sys-color-outline) 15%, transparent) 75%),
    linear-gradient(-45deg, transparent 75%, color-mix(in srgb, var(--md-sys-color-outline) 15%, transparent) 75%);
  background-size: 20px 20px;
  background-position: 0 0, 0 10px, 10px -10px, -10px 0;
}

@media (min-width: 640px) { .canvas-area { padding: 24px; } }

.fullscreen-pad { padding: 16px; }

.canvas-wrap {
  position: relative;
  overflow: hidden;
  border: 1px solid var(--md-sys-color-outline-variant);
  user-select: none;
  margin: 0 auto;
  transition: opacity 0.2s, max-width 0.2s ease-out;
  box-shadow: 0 1px 2px rgba(0,0,0,0.1);
}

.main-canvas {
  width: 100%;
  height: 100%;
  display: block;
}

.inline-edit-text {
  position: absolute;
  z-index: 20;
  border: none;
  outline: none;
  resize: none;
  padding: 0;
  margin: 0;
  background: transparent;
  border-radius: 2px;
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--md-sys-color-primary) 50%, transparent);
  overflow: hidden;
}

.inline-edit-sticker {
  position: absolute;
  z-index: 20;
  border: none;
  outline: none;
  resize: none;
  padding: 0;
  margin: 0;
}

.overlay-canvas {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
}

.zoom-indicator {
  position: absolute;
  bottom: 12px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 10;
}

.zoom-label {
  display: inline-block;
  font-size: 12px;
  font-weight: 500;
  color: var(--md-sys-color-on-surface-variant);
  background: var(--md-sys-color-surface-container-high);
  padding: 4px 10px;
  border-radius: 8px;
  border: 1px solid var(--md-sys-color-outline-variant);
  font-variant-numeric: tabular-nums;
  cursor: pointer;
  box-shadow: 0 1px 2px rgba(0,0,0,0.1);
  &:hover { color: var(--md-sys-color-on-surface); }
}

.sidebar {
  border-top: 1px solid var(--md-sys-color-outline-variant);
  overflow-y: auto;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  background: color-mix(in srgb, var(--md-sys-color-surface-container-high) 60%, transparent);
}

@media (min-width: 1024px) {
  .sidebar {
    border-top: none;
    border-left: 1px solid var(--md-sys-color-outline-variant);
  }
}

.cropping-dim {
  opacity: 0.4;
  pointer-events: none;
}

.divider {
  border: none;
  border-top: 1px solid var(--md-sys-color-outline-variant);
  margin: 0;
}

.actions-section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.action-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  width: 100%;
  padding: 10px 16px;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 20px;
  background: transparent;
  color: var(--md-sys-color-on-surface);
  font-family: inherit;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;

  &.outline:hover {
    border-color: var(--md-sys-color-primary);
    color: var(--md-sys-color-primary);
  }
  &.danger {
    color: var(--md-sys-color-error);
    border-color: color-mix(in srgb, var(--md-sys-color-error) 20%, transparent);
    &:hover {
      background: color-mix(in srgb, var(--md-sys-color-error) 10%, transparent);
      border-color: color-mix(in srgb, var(--md-sys-color-error) 30%, transparent);
    }
  }
}
</style>
