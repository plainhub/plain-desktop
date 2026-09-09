<template>
  <div class="capture-toolbar" role="toolbar" :aria-label="messages.a11y.toolbar" @pointerdown.stop @dblclick.stop @contextmenu.stop.prevent>
    <div class="capture-toolbar__group" :aria-label="messages.a11y.annotationTools">
      <button
        v-for="tool in tools"
        :key="tool.id"
        type="button"
        class="capture-toolbar__button"
        :class="{ active: activeTool === tool.id }"
        :data-tool="tool.id"
        :aria-label="tool.label"
        :title="tool.label"
        :aria-pressed="activeTool === tool.id"
        :disabled="busy"
        @click="$emit('tool', tool.id)"
      >
        <span aria-hidden="true">{{ tool.icon }}</span>
      </button>
    </div>

    <span class="capture-toolbar__divider" aria-hidden="true" />

    <div class="capture-toolbar__group" :aria-label="messages.a11y.history">
      <button type="button" class="capture-toolbar__button" :aria-label="messages.actions.undo" :title="messages.actions.undo" :disabled="busy || !canUndo" @click="$emit('undo')">↶</button>
      <button type="button" class="capture-toolbar__button" :aria-label="messages.actions.redo" :title="messages.actions.redo" :disabled="busy || !canRedo" @click="$emit('redo')">↷</button>
    </div>

    <div class="capture-toolbar__group capture-toolbar__colors" :aria-label="messages.a11y.colors">
      <button
        v-for="preset in colors"
        :key="preset"
        type="button"
        class="capture-toolbar__color"
        :class="{ active: color === preset }"
        :style="{ '--capture-color': preset }"
        :data-color="preset"
        :aria-label="colorLabel(preset)"
        :title="colorLabel(preset)"
        :aria-pressed="color === preset"
        :disabled="busy"
        @click="$emit('color', preset)"
      />
    </div>

    <div class="capture-toolbar__group" :aria-label="messages.a11y.strokeWidth">
      <button
        v-for="width in strokeWidths"
        :key="width"
        type="button"
        class="capture-toolbar__stroke"
        :class="{ active: strokeWidth === width }"
        :data-stroke-width="width"
        :aria-label="strokeWidthLabel(width)"
        :title="strokeWidthLabel(width)"
        :aria-pressed="strokeWidth === width"
        :disabled="busy"
        @click="$emit('stroke-width', width)"
      >
        <span :style="{ height: `${Math.max(2, width / 2)}px` }" aria-hidden="true" />
      </button>
    </div>

    <span class="capture-toolbar__divider" aria-hidden="true" />

    <div class="capture-toolbar__group" :aria-label="messages.a11y.actions">
      <button type="button" class="capture-toolbar__button" data-action="save" :aria-label="messages.actions.save" :title="messages.actions.save" :disabled="busy" @click="$emit('action', 'save')">
        ⇩
      </button>
      <button type="button" class="capture-toolbar__button" data-action="copy" :aria-label="messages.actions.copy" :title="messages.actions.copy" :disabled="busy" @click="$emit('action', 'copy')">
        ⧉
      </button>
      <button type="button" class="capture-toolbar__button" data-action="cancel" :aria-label="messages.actions.cancel" :title="messages.actions.cancel" :disabled="busy" @click="$emit('cancel')">
        ×
      </button>
      <span
        class="capture-toolbar__tooltip-host"
        data-testid="capture-confirm-tooltip-host"
        :tabindex="!canConfirm && !busy ? 0 : undefined"
      >
        <button
          type="button"
          class="capture-toolbar__button capture-toolbar__confirm"
          data-action="confirm"
          :aria-label="messages.actions.confirm"
          :aria-describedby="!canConfirm ? 'capture-confirm-disabled-reason' : undefined"
          :title="canConfirm ? messages.actions.confirm : messages.actions.openChatToSend"
          :disabled="busy || !canConfirm"
          @click="$emit('action', 'confirm')"
        >
          ✓
        </button>
        <span v-if="!canConfirm" id="capture-confirm-disabled-reason" class="capture-toolbar__tooltip" role="tooltip">
          {{ messages.actions.openChatToSend }}
        </span>
      </span>
    </div>
  </div>
</template>

<script lang="ts">
export type CaptureAnnotationTool = 'rect' | 'ellipse' | 'arrow' | 'brush' | 'text' | 'mosaic'
export type CaptureExportAction = 'save' | 'copy' | 'confirm'
</script>

<script setup lang="ts">
import { computed } from 'vue'
import { defaultCaptureMessages, formatCaptureMessage, type CaptureMessages } from './capture-localization'

interface Props {
  activeTool: CaptureAnnotationTool | null
  color: string
  strokeWidth: number
  canUndo: boolean
  canRedo: boolean
  busy: boolean
  canConfirm: boolean
  messages?: CaptureMessages
}

const props = withDefaults(defineProps<Props>(), {
  messages: () => defaultCaptureMessages,
})

defineEmits<{
  tool: [tool: CaptureAnnotationTool]
  color: [color: string]
  'stroke-width': [width: number]
  undo: []
  redo: []
  action: [action: CaptureExportAction]
  cancel: []
}>()

const tools = computed<ReadonlyArray<{ id: CaptureAnnotationTool; label: string; icon: string }>>(() => [
  { id: 'rect', label: props.messages.tools.rectangle, icon: '□' },
  { id: 'ellipse', label: props.messages.tools.ellipse, icon: '○' },
  { id: 'arrow', label: props.messages.tools.arrow, icon: '↗' },
  { id: 'brush', label: props.messages.tools.pen, icon: '⌁' },
  { id: 'text', label: props.messages.tools.text, icon: 'T' },
  { id: 'mosaic', label: props.messages.tools.mosaic, icon: '▦' },
])

const colors = ['#ef4444', '#eab308', '#22c55e', '#3b82f6', '#000000'] as const
const strokeWidths = [2, 4, 8] as const

function colorLabel(color: string): string {
  return formatCaptureMessage(props.messages.a11y.color, { color })
}

function strokeWidthLabel(width: number): string {
  return formatCaptureMessage(props.messages.a11y.strokeWidthOption, { width })
}
</script>

<style scoped>
.capture-toolbar {
  display: flex;
  align-items: center;
  gap: 5px;
  min-height: 44px;
  padding: 6px;
  color: #f8fafc;
  background: rgb(24 24 27 / 96%);
  border: 1px solid rgb(255 255 255 / 16%);
  border-radius: 9px;
  box-shadow: 0 8px 30px rgb(0 0 0 / 42%);
  user-select: none;
}

.capture-toolbar__group {
  display: flex;
  align-items: center;
  gap: 3px;
}

.capture-toolbar__button,
.capture-toolbar__stroke,
.capture-toolbar__color {
  display: grid;
  place-items: center;
  box-sizing: border-box;
  color: inherit;
  background: transparent;
  border: 0;
  border-radius: 5px;
  cursor: pointer;
}

.capture-toolbar__button,
.capture-toolbar__stroke {
  width: 32px;
  height: 32px;
  font:
    600 17px/1 system-ui,
    sans-serif;
}

.capture-toolbar__button:hover:not(:disabled),
.capture-toolbar__button.active,
.capture-toolbar__stroke:hover:not(:disabled),
.capture-toolbar__stroke.active {
  background: rgb(255 255 255 / 14%);
}

.capture-toolbar__button:focus-visible,
.capture-toolbar__stroke:focus-visible,
.capture-toolbar__color:focus-visible {
  outline: 2px solid #60a5fa;
  outline-offset: 1px;
}

.capture-toolbar__button:disabled,
.capture-toolbar__stroke:disabled,
.capture-toolbar__color:disabled {
  cursor: default;
  opacity: 0.38;
}

.capture-toolbar__divider {
  width: 1px;
  height: 24px;
  margin: 0 2px;
  background: rgb(255 255 255 / 18%);
}

.capture-toolbar__colors {
  padding: 0 2px;
}

.capture-toolbar__color {
  width: 20px;
  height: 20px;
  margin: 0 1px;
  background: var(--capture-color);
  border: 2px solid transparent;
  border-radius: 50%;
}

.capture-toolbar__color.active {
  border-color: #fff;
  box-shadow: 0 0 0 1px #111827;
}

.capture-toolbar__stroke span {
  display: block;
  width: 19px;
  max-height: 8px;
  background: currentcolor;
  border-radius: 999px;
}

.capture-toolbar__confirm {
  color: #052e16;
  background: #4ade80;
}

.capture-toolbar__confirm:hover:not(:disabled) {
  background: #86efac;
}

.capture-toolbar__tooltip-host {
  position: relative;
  display: grid;
  place-items: center;
  border-radius: 5px;
  outline: none;
}

.capture-toolbar__tooltip-host:focus-visible {
  outline: 2px solid #60a5fa;
  outline-offset: 1px;
}

.capture-toolbar__tooltip {
  position: absolute;
  right: 0;
  bottom: calc(100% + 8px);
  z-index: 10;
  width: max-content;
  max-width: 240px;
  padding: 6px 9px;
  color: #f8fafc;
  font: 500 12px/1.35 system-ui, sans-serif;
  text-align: left;
  white-space: normal;
  pointer-events: none;
  visibility: hidden;
  opacity: 0;
  background: rgb(24 24 27 / 98%);
  border: 1px solid rgb(255 255 255 / 18%);
  border-radius: 6px;
  box-shadow: 0 5px 18px rgb(0 0 0 / 42%);
  transition:
    opacity 120ms ease,
    visibility 120ms ease;
}

.capture-toolbar__tooltip-host:hover .capture-toolbar__tooltip,
.capture-toolbar__tooltip-host:focus-visible .capture-toolbar__tooltip {
  visibility: visible;
  opacity: 1;
}
</style>
