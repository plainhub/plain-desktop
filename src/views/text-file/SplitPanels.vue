<template>
  <div ref="containerRef" class="split-panels" :data-folded="folded ?? 'none'">
    <div v-if="folded === 'tree'" class="pane source-pane" style="flex: 1 1 100%">
      <slot name="source" />
    </div>
    <div v-else-if="!folded" class="pane source-pane" :style="{ flex: `1 1 ${ratio * 100}%` }">
      <slot name="source" />
    </div>

    <div
      v-if="!folded"
      class="split-handle"
      @mousedown.prevent="startDrag"
      @touchstart.prevent="startDrag"
    >
      <div class="bar" />
    </div>

    <div v-if="folded !== 'tree'" class="pane tree-pane" :style="folded === 'source' ? { flex: '1 1 100%' } : { flex: `1 1 ${100 - ratio * 100}%` }">
      <slot name="tree" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

const props = defineProps<{
  ratio: number
  folded: 'source' | 'tree' | null
}>()

const emit = defineEmits<{
  'update:ratio': [ratio: number]
}>()

const containerRef = ref<HTMLElement>()
let dragging = false

function startDrag() {
  dragging = true
  window.addEventListener('mousemove', onDrag)
  window.addEventListener('mouseup', stopDrag)
  window.addEventListener('touchmove', onTouchDrag)
  window.addEventListener('touchend', stopDrag)
}

function onDrag(e: MouseEvent) {
  if (!dragging || !containerRef.value) return
  const rect = containerRef.value.getBoundingClientRect()
  const next = Math.min(0.8, Math.max(0.2, (e.clientX - rect.left) / rect.width))
  emit('update:ratio', next)
}

function onTouchDrag(e: TouchEvent) {
  if (!dragging || !containerRef.value || !e.touches.length) return
  const rect = containerRef.value.getBoundingClientRect()
  const next = Math.min(0.8, Math.max(0.2, (e.touches[0].clientX - rect.left) / rect.width))
  emit('update:ratio', next)
}

function stopDrag() {
  dragging = false
  window.removeEventListener('mousemove', onDrag)
  window.removeEventListener('mouseup', stopDrag)
  window.removeEventListener('touchmove', onTouchDrag)
  window.removeEventListener('touchend', stopDrag)
}
</script>

<style lang="scss" scoped>
.split-panels {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: 0;
  width: 100%;
}

.pane {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;

  > :deep(*) {
    flex: 1;
    min-height: 0;
  }
}

.split-handle {
  width: 8px;
  flex-shrink: 0;
  cursor: col-resize;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;

  &:hover {
    background: color-mix(in srgb, var(--md-sys-color-primary) 8%, transparent);
  }

  .bar {
    width: 2px;
    height: 32px;
    border-radius: 2px;
    background: var(--md-sys-color-outline-variant);
  }
}

@media (max-width: 768px) {
  .split-handle {
    display: none !important;
  }

  .pane {
    flex: 1 1 100% !important;
  }

  .split-panels[data-folded="none"] .tree-pane,
  .split-panels[data-folded="tree"] .tree-pane {
    display: none !important;
  }

  .split-panels[data-folded="source"] .source-pane {
    display: none !important;
  }
}
</style>
