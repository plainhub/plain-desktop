<template>
  <div>
    <div class="header">
      <label class="section-label">{{ $t('image_editor.background') }}</label>
      <div class="mode-toggle">
        <button
          v-tooltip="$t('image_editor.bg_color_mode')"
          class="mode-btn"
          :class="{ active: bgMode === 'color' }"
          @click="bgMode = 'color'"
        >
          <i-lucide-palette />
        </button>
        <button
          v-tooltip="$t('image_editor.bg_image_mode')"
          class="mode-btn"
          :class="{ active: bgMode === 'image' }"
          @click="bgMode = 'image'"
        >
          <i-lucide-image />
        </button>
      </div>
    </div>

    <div v-if="bgMode === 'color'" class="color-mode">
      <input v-model="bgColor" type="color" class="color-input" />
      <div class="swatches">
        <button
          v-for="c in PRESET_COLORS"
          :key="c"
          class="swatch"
          :class="{ active: bgColor === c }"
          :style="{ backgroundColor: c }"
          @click="bgColor = c"
        />
      </div>
    </div>

    <template v-else>
      <div v-if="!photoSrc" class="upload-area">
        <label class="upload-btn">
          <i-lucide-image />
          {{ $t('image_editor.add_image') }}
          <input type="file" accept="image/*" class="hidden-input" @change="$emit('photoSelect', $event)" />
        </label>
      </div>
      <div v-else class="image-mode">
        <div class="image-row">
          <img :src="photoSrc" class="thumb" alt="" />
          <label class="replace-btn">
            <i-lucide-arrow-left-right />
            {{ $t('image_editor.replace_image') }}
            <input type="file" accept="image/*" class="hidden-input" @change="$emit('photoSelect', $event)" />
          </label>
          <button v-tooltip="$t('image_editor.delete')" class="remove-btn" @click="$emit('removePhoto')">
            <i-lucide-circle-x />
          </button>
        </div>
        <div class="row">
          <label class="slider-label">{{ $t('image_editor.opacity') }}</label>
          <input v-model.number="bgImageAlpha" type="range" min="0" max="100" class="slider" />
          <span class="value">{{ bgImageAlpha }}%</span>
        </div>
        <button
          v-if="bgOffsetChanged"
          class="reset-offset-btn"
          @click="$emit('resetBgOffset')"
        >
          <i-lucide-rotate-ccw />
          {{ $t('image_editor.reset_position') }}
        </button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const PRESET_COLORS = ['#ffffff', '#000000', '#f5f5f5', '#1f2937', '#3b82f6', '#22c55e', '#ef4444', '#fbbf24']

const bgMode = defineModel<'color' | 'image'>('bgMode', { required: true })
const bgColor = defineModel<string>('bgColor', { required: true })
const bgImageAlpha = defineModel<number>('bgImageAlpha', { required: true })

const props = defineProps<{
  photoSrc: string | null
  bgOffset?: { x: number; y: number }
}>()

defineEmits<{
  photoSelect: [e: Event]
  removePhoto: []
  resetBgOffset: []
}>()

const bgOffsetChanged = computed(() => {
  if (!props.bgOffset) return false
  return props.bgOffset.x !== 0 || props.bgOffset.y !== 0
})
</script>

<style lang="scss" scoped>
.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
}

.section-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--md-sys-color-on-surface);
}

.mode-toggle {
  display: flex;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 8px;
  overflow: hidden;
}

.mode-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 6px 10px;
  border: none;
  background: var(--md-sys-color-surface-container-high);
  color: var(--md-sys-color-on-surface-variant);
  cursor: pointer;
  font-size: 14px;
  transition: all 0.15s;

  &.active {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }
  & + & { border-left: 1px solid var(--md-sys-color-outline-variant); }
}

.color-mode {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.color-input {
  width: 36px;
  height: 36px;
  padding: 0;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 8px;
  background: none;
  cursor: pointer;
}

.swatches {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.swatch {
  width: 22px;
  height: 22px;
  border: 2px solid var(--md-sys-color-outline-variant);
  border-radius: 50%;
  cursor: pointer;
  padding: 0;
  transition: transform 0.1s, border-color 0.1s;

  &.active {
    border-color: var(--md-sys-color-primary);
    transform: scale(1.1);
  }
}

.hidden-input { display: none; }

.upload-area { display: flex; gap: 8px; }

.upload-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 12px;
  border: 2px dashed var(--md-sys-color-outline);
  border-radius: 10px;
  background: var(--md-sys-color-surface-container-high);
  color: var(--md-sys-color-on-surface-variant);
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: border-color 0.15s, color 0.15s;
  &:hover {
    border-color: var(--md-sys-color-primary);
    color: var(--md-sys-color-primary);
  }
}

.image-mode {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.image-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 10px;
  background: var(--md-sys-color-surface-container-high);
}

.thumb {
  width: 48px;
  height: 48px;
  border-radius: 6px;
  object-fit: cover;
}

.replace-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 8px;
  background: var(--md-sys-color-surface);
  color: var(--md-sys-color-on-surface-variant);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  transition: all 0.15s;
  &:hover {
    border-color: var(--md-sys-color-primary);
    color: var(--md-sys-color-primary);
  }
}

.remove-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--md-sys-color-error);
  cursor: pointer;
  font-size: 20px;
  padding: 4px;
  &:hover { filter: brightness(1.2); }
}

.row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.slider-label {
  font-size: 12px;
  color: var(--md-sys-color-on-surface-variant);
  flex-shrink: 0;
}

.slider {
  flex: 1;
  accent-color: var(--md-sys-color-primary);
}

.value {
  font-size: 11px;
  color: var(--md-sys-color-on-surface-variant);
  width: 32px;
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.reset-offset-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  width: 100%;
  padding: 6px 12px;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 8px;
  background: var(--md-sys-color-surface-container-high);
  color: var(--md-sys-color-on-surface-variant);
  font-family: inherit;
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
  &:hover { color: var(--md-sys-color-on-surface); }
}
</style>
