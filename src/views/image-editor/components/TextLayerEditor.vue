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
          <span class="title">{{ $t('image_editor.text_style') }}</span>
          <button v-tooltip="$t('image_editor.close')" class="close-btn" @click="$emit('close')">
            <i-lucide-x />
          </button>
        </div>

        <select :value="layer.fontFamily" class="font-select" @change="setProperty('fontFamily', ($event.target as HTMLSelectElement).value)">
          <option v-for="f in FONT_FAMILIES" :key="f.value" :value="f.value">{{ f.label }}</option>
        </select>

        <div class="row">
          <input :value="layer.color" type="color" class="color-input" @input="setProperty('color', ($event.target as HTMLInputElement).value)" />
          <input :value="layer.fontSize" type="range" min="14" max="200" class="slider" @input="setProperty('fontSize', ($event.target as HTMLInputElement).valueAsNumber)" />
          <span class="value">{{ layer.fontSize }}</span>
        </div>

        <div class="toggles">
          <button
            v-for="a in Object.values(EditorTextAlign)"
            :key="a"
            class="toggle-btn"
            :class="{ active: layer.textAlign === a }"
            @click="setProperty('textAlign', a)"
          >
            <component :is="ALIGN_ICONS[a]" />
          </button>
          <div class="divider" />
          <button
            class="toggle-btn bold"
            :class="{ active: layer.fontWeight === EditorFontWeight.BOLD }"
            @click="setProperty('fontWeight', layer.fontWeight === EditorFontWeight.BOLD ? EditorFontWeight.NORMAL : EditorFontWeight.BOLD)"
          >B</button>
          <button
            class="toggle-btn italic"
            :class="{ active: layer.fontStyle === EditorFontStyle.ITALIC }"
            @click="setProperty('fontStyle', layer.fontStyle === EditorFontStyle.ITALIC ? EditorFontStyle.NORMAL : EditorFontStyle.ITALIC)"
          >I</button>
          <button
            class="toggle-btn underline"
            :class="{ active: layer.textDecoration === EditorTextDecoration.UNDERLINE }"
            @click="setProperty('textDecoration', layer.textDecoration === EditorTextDecoration.UNDERLINE ? EditorTextDecoration.NONE : EditorTextDecoration.UNDERLINE)"
          >U</button>
        </div>

        <div>
          <button class="collapse-btn" @click="showStroke = !showStroke">
            <i-lucide-chevron-down v-if="showStroke" />
            <i-lucide-chevron-right v-else />
            <span class="collapse-label">{{ $t('image_editor.border_width') }}</span>
            <span v-if="layer.textStroke.width > 0" class="collapse-value">{{ layer.textStroke.width }}px</span>
          </button>
          <div v-if="showStroke" class="collapse-body">
            <div class="row">
              <input :value="layer.textStroke.color" type="color" class="color-input" @input="setStroke('color', ($event.target as HTMLInputElement).value)" />
              <input :value="layer.textStroke.width" type="range" min="0" max="16" class="slider" @input="setStroke('width', ($event.target as HTMLInputElement).valueAsNumber)" />
              <span class="value">{{ layer.textStroke.width }}</span>
            </div>
          </div>
        </div>

        <div>
          <button class="collapse-btn" @click="showShadow = !showShadow">
            <i-lucide-chevron-down v-if="showShadow" />
            <i-lucide-chevron-right v-else />
            <span class="collapse-label">{{ $t('image_editor.shadow') }}</span>
            <span v-if="hasShadow" class="collapse-dot" />
          </button>
          <div v-if="showShadow" class="collapse-body">
            <div class="presets">
              <button
                v-for="preset in shadowPresets"
                :key="preset.label"
                class="preset-btn"
                :class="{ active: isShadowPresetActive(preset) }"
                @click="applyShadowPreset(preset)"
              >{{ preset.label }}</button>
            </div>
            <div class="row">
              <input :value="layer.textShadow.color" type="color" class="color-input" @input="setShadow('color', ($event.target as HTMLInputElement).value)" />
              <label class="slider-label">{{ $t('image_editor.shadow_blur') }}</label>
              <input :value="layer.textShadow.blur" type="range" min="0" max="40" class="slider" @input="setShadow('blur', ($event.target as HTMLInputElement).valueAsNumber)" />
              <span class="value">{{ layer.textShadow.blur }}</span>
            </div>
            <div class="row">
              <label class="slider-label">X</label>
              <input :value="layer.textShadow.offsetX" type="range" min="-20" max="20" class="slider" @input="setShadow('offsetX', ($event.target as HTMLInputElement).valueAsNumber)" />
              <span class="value">{{ layer.textShadow.offsetX }}</span>
            </div>
            <div class="row">
              <label class="slider-label">Y</label>
              <input :value="layer.textShadow.offsetY" type="range" min="-20" max="20" class="slider" @input="setShadow('offsetY', ($event.target as HTMLInputElement).valueAsNumber)" />
              <span class="value">{{ layer.textShadow.offsetY }}</span>
            </div>
          </div>
        </div>

        <div class="glow-row">
          <button
            class="glow-btn"
            :class="{ active: layer.glow }"
            @click="setProperty('glow', !layer.glow)"
          >
            <i-lucide-sun />
            {{ $t('image_editor.glow') }}
          </button>
        </div>

        <div>
          <button class="collapse-btn" @click="showMore = !showMore">
            <i-lucide-chevron-down v-if="showMore" />
            <i-lucide-chevron-right v-else />
            <span class="collapse-label">{{ $t('image_editor.more_options') }}</span>
          </button>
          <div v-if="showMore" class="collapse-body">
            <div class="row">
              <label class="slider-label wide">{{ $t('image_editor.rotation') }}</label>
              <input :value="layer.rotation" type="range" min="-180" max="180" class="slider" @input="setProperty('rotation', ($event.target as HTMLInputElement).valueAsNumber)" />
              <span class="value">{{ layer.rotation }}°</span>
              <button v-if="layer.rotation !== 0" class="reset-btn" @click="setProperty('rotation', 0)">
                {{ $t('image_editor.reset') }}
              </button>
            </div>
            <div class="row">
              <label class="slider-label wide">{{ $t('image_editor.letter_spacing') }}</label>
              <input :value="layer.letterSpacing" type="range" min="-5" max="30" class="slider" @input="setProperty('letterSpacing', ($event.target as HTMLInputElement).valueAsNumber)" />
              <span class="value">{{ layer.letterSpacing }}</span>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import AlignLeft from '~icons/lucide/align-left'
import AlignCenter from '~icons/lucide/align-center'
import AlignRight from '~icons/lucide/align-right'
import { FONT_FAMILIES } from '@/views/image-editor/data/font-families'
import { useTextLayerEditor } from '@/views/image-editor/composables/useTextLayerEditor'
import { EditorFontWeight, EditorFontStyle, EditorTextAlign, EditorTextDecoration, type EditorTextLayer } from '@/views/image-editor/utils/types'
import type { ImageEditorDoc } from '@/views/image-editor/composables/useImageEditorDoc'

const ALIGN_ICONS = { [EditorTextAlign.LEFT]: AlignLeft, [EditorTextAlign.CENTER]: AlignCenter, [EditorTextAlign.RIGHT]: AlignRight }

const props = defineProps<{
  layer: EditorTextLayer | null
  rect: { top: number; left: number }
  doc: ImageEditorDoc
}>()

const emit = defineEmits<{ close: [] }>()

const {
  panelRef, posX, posY, onDragStart,
  showStroke, showShadow, showMore, hasShadow,
  shadowPresets, applyShadowPreset, isShadowPresetActive, setProperty, setStroke, setShadow,
} = useTextLayerEditor(
  () => props.layer,
  () => props.rect,
  props.doc,
  () => emit('close'),
)
</script>

<style lang="scss" scoped>
.panel {
  position: fixed;
  z-index: 102;
  width: 320px;
  box-sizing: border-box;
  max-width: calc(100% - 16px);
  max-height: 80vh;
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

.font-select {
  width: 100%;
  padding: 6px 10px;
  font-size: 12px;
  font-family: inherit;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 8px;
  background: var(--md-sys-color-surface);
  color: var(--md-sys-color-on-surface);
}

.row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.color-input {
  width: 28px;
  height: 28px;
  padding: 0;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 6px;
  background: none;
  cursor: pointer;
  flex-shrink: 0;
}

.slider {
  flex: 1;
  accent-color: var(--md-sys-color-primary);
}

.slider-label {
  font-size: 11px;
  color: var(--md-sys-color-on-surface-variant);
  flex-shrink: 0;
  &.wide { width: 40px; }
}

.value {
  font-size: 11px;
  color: var(--md-sys-color-on-surface-variant);
  width: 24px;
  text-align: right;
  font-variant-numeric: tabular-nums;
  flex-shrink: 0;
}

.toggles {
  display: flex;
  align-items: center;
  gap: 2px;
}

.toggle-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 8px;
  background: var(--md-sys-color-surface);
  color: var(--md-sys-color-on-surface-variant);
  font-family: inherit;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s;

  &.bold { font-weight: 700; }
  &.italic { font-style: italic; }
  &.underline { text-decoration: underline; }

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

.collapse-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 6px 4px;
  border: none;
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  font-family: inherit;
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  border-radius: 6px;
  &:hover { color: var(--md-sys-color-on-surface); }
}

.collapse-label { flex: 1; }

.collapse-value {
  font-size: 10px;
  color: var(--md-sys-color-primary);
  font-variant-numeric: tabular-nums;
}

.collapse-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--md-sys-color-primary);
}

.collapse-body {
  margin-top: 8px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-left: 4px;
}

.presets {
  display: flex;
  gap: 6px;
}

.preset-btn {
  flex: 1;
  padding: 4px 8px;
  font-size: 10px;
  font-weight: 500;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 6px;
  background: var(--md-sys-color-surface);
  color: var(--md-sys-color-on-surface-variant);
  font-family: inherit;
  cursor: pointer;
  transition: all 0.15s;

  &.active {
    background: color-mix(in srgb, var(--md-sys-color-primary) 20%, transparent);
    border-color: color-mix(in srgb, var(--md-sys-color-primary) 40%, transparent);
    color: var(--md-sys-color-primary);
  }
}

.glow-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.glow-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  font-size: 11px;
  font-weight: 500;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 8px;
  background: var(--md-sys-color-surface);
  color: var(--md-sys-color-on-surface-variant);
  font-family: inherit;
  cursor: pointer;
  transition: all 0.15s;

  &.active {
    background: color-mix(in srgb, var(--md-sys-color-primary) 20%, transparent);
    border-color: color-mix(in srgb, var(--md-sys-color-primary) 40%, transparent);
    color: var(--md-sys-color-primary);
  }
}

.reset-btn {
  border: none;
  background: transparent;
  color: var(--md-sys-color-primary);
  font-family: inherit;
  font-size: 10px;
  cursor: pointer;
  &:hover { filter: brightness(1.2); }
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
