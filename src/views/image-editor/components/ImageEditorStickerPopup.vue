<template>
  <Teleport to="body">
    <Transition name="popup">
      <div
        v-if="layer"
        ref="panelRef"
        class="panel"
        :style="{ top: posY + 'px', left: posX + 'px' }"
      >
        <div class="header" @pointerdown.prevent="onDragStart">
          <span class="title">{{ $t('image_editor.tool_sticker') }}</span>
          <button v-tooltip="$t('image_editor.close')" class="close-btn" @click="$emit('close')">
            <i-lucide-x />
          </button>
        </div>

        <div class="row">
          <span class="label">Aa</span>
          <input :value="layer.fontSize" type="range" min="10" max="120" class="slider" @input="onFontSize" />
          <span class="value">{{ layer.fontSize }}</span>
        </div>

        <div class="toggles">
          <button
            class="toggle-btn bold"
            :class="{ active: layer.fontWeight === EditorFontWeight.BOLD }"
            @click="toggleBold"
          >B</button>
          <button
            class="toggle-btn italic"
            :class="{ active: layer.fontStyle === EditorFontStyle.ITALIC }"
            @click="toggleItalic"
          >I</button>
          <div class="divider" />
          <button
            v-for="c in COLORS"
            :key="c"
            class="swatch"
            :class="{ active: layer.color === c }"
            :style="{ backgroundColor: c }"
            @click="onColor(c)"
          />
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { useEditorPopup } from '../composables/useEditorPopup'
import { EditorFontWeight, EditorFontStyle, type StickerLayer } from '@/views/image-editor/utils/types'
import { measureStickerSize } from '@/views/image-editor/utils/editor-draw-layers'
import type { ImageEditorDoc } from '@/views/image-editor/composables/useImageEditorDoc'

const COLORS = ['#fef08a', '#bbf7d0', '#bfdbfe', '#fbcfe8', '#fde68a']

const props = defineProps<{
  layer: StickerLayer | null
  rect: { top: number; left: number }
  doc: ImageEditorDoc
}>()

const emit = defineEmits<{ close: [] }>()

const { panelRef, posX, posY, onDragStart } = useEditorPopup(() => !!props.layer, () => props.rect, 256, () => emit('close'))

function resizeFor(layer: StickerLayer) {
  const { w, h } = measureStickerSize(layer)
  props.doc.setLayerProps(layer.id, { w, h })
}

function onFontSize(e: Event) {
  if (!props.layer) return
  const size = Number((e.target as HTMLInputElement).value)
  const updated = { ...props.layer, fontSize: size }
  props.doc.setLayerProps(props.layer.id, { fontSize: size })
  resizeFor(updated)
}

function onColor(c: string) {
  if (!props.layer) return
  const updated = { ...props.layer, color: c }
  props.doc.setLayerProps(props.layer.id, { color: c })
  resizeFor(updated)
}

function toggleBold() {
  if (!props.layer) return
  const next = props.layer.fontWeight === EditorFontWeight.BOLD ? EditorFontWeight.SEMIBOLD : EditorFontWeight.BOLD
  const updated = { ...props.layer, fontWeight: next }
  props.doc.setLayerProp(props.layer.id, 'fontWeight', next)
  resizeFor(updated)
}

function toggleItalic() {
  if (!props.layer) return
  const next = props.layer.fontStyle === EditorFontStyle.ITALIC ? EditorFontStyle.NORMAL : EditorFontStyle.ITALIC
  props.doc.setLayerProp(props.layer.id, 'fontStyle', next)
}

</script>

<style lang="scss" scoped>
.panel {
  position: fixed;
  z-index: 102;
  width: 256px;
  box-sizing: border-box;
  max-width: calc(100% - 16px);
  max-height: calc(100vh - 16px);
  overflow-y: auto;
  background: var(--md-sys-color-surface-container-high);
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 12px;
  box-shadow: 0 20px 25px -5px rgba(0,0,0,0.3);
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: move;
  user-select: none;
}

.title {
  font-size: 12px;
  font-weight: 600;
  color: var(--md-sys-color-on-surface);
}

.close-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  cursor: pointer;
  padding: 2px;
  border-radius: 4px;
  font-size: 16px;
  &:hover { color: var(--md-sys-color-on-surface); }
}

.row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.label {
  font-size: 10px;
  text-transform: uppercase;
  font-weight: 600;
  color: var(--md-sys-color-on-surface-variant);
  flex-shrink: 0;
}

.slider {
  flex: 1;
  accent-color: var(--md-sys-color-primary);
}

.value {
  font-size: 10px;
  color: var(--md-sys-color-on-surface-variant);
  width: 24px;
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.toggles {
  display: flex;
  align-items: center;
  gap: 4px;
}

.toggle-btn {
  padding: 6px 10px;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 8px;
  background: var(--md-sys-color-surface);
  color: var(--md-sys-color-on-surface-variant);
  font-family: inherit;
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;

  &.bold { font-weight: 700; }
  &.italic { font-style: italic; }

  &.active {
    background: color-mix(in srgb, var(--md-sys-color-primary) 20%, transparent);
    border-color: color-mix(in srgb, var(--md-sys-color-primary) 40%, transparent);
    color: var(--md-sys-color-primary);
  }
}

.divider {
  width: 1px;
  height: 20px;
  background: var(--md-sys-color-outline-variant);
  margin: 0 4px;
}

.swatch {
  width: 24px;
  height: 24px;
  border: 2px solid var(--md-sys-color-outline-variant);
  border-radius: 6px;
  cursor: pointer;
  flex-shrink: 0;
  transition: transform 0.1s, border-color 0.1s;
  &.active {
    border-color: var(--md-sys-color-primary);
    transform: scale(1.1);
  }
}

.popup-enter-active, .popup-leave-active {
  transition: opacity 0.15s, transform 0.15s;
  transform-origin: top right;
}
.popup-enter-from, .popup-leave-to {
  opacity: 0;
  transform: scale(0.95);
}
</style>
