<template>
  <div ref="containerRef" class="v-dropdown-container" :class="{ 'is-block': fullWidth }">
    <div @click.prevent.stop="toggle">
      <slot name="trigger" />
    </div>
    <teleport to="body">
      <div
        v-if="modelValue"
        ref="menuRef"
        class="v-dropdown-portal dropdown-menu is-open"
        :class="{ borderless }"
        :style="style"
      >
        <slot />
      </div>
    </teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, nextTick, onMounted, onUnmounted } from 'vue'
import type { CSSProperties } from 'vue'

const props = defineProps({
  modelValue: { type: Boolean, default: false },
  borderless: { type: Boolean, default: false },
  maxHeight: { type: Number, default: 400 },
  // Kept for backward compatibility — positioning is now fully automatic.
  align: { type: String, default: '' },
  // 'auto': prefer beside trigger; 'below': always place below/above
  strategy: { type: String, default: 'auto' },
  // Make the container block-level (full parent width)
  fullWidth: { type: Boolean, default: false },
})

const emit = defineEmits(['update:modelValue'])

const EDGE_MARGIN = 16 // minimum clearance from viewport edges
const TRIGGER_GAP = 8  // gap between trigger element and menu
const containerRef = ref<HTMLElement | null>(null)
const menuRef = ref<HTMLElement | null>(null)
const style = ref<CSSProperties>({ position: 'fixed', visibility: 'hidden' })

/**
 * Compute the optimal position for the dropdown menu.
 *
 * Strategy:
 *   1. Prefer placing the menu BESIDE the trigger (left or right) when there
 *      is enough horizontal room for the menu width.
 *   2. In horizontal placement the vertical anchor is chosen independently:
 *      - Enough space below trigger → open DOWNWARD (top = trigger.bottom)
 *      - Enough space above trigger → open UPWARD  (bottom = trigger.top)
 *      - Else use whichever side has more room.
 *      This gives true diagonal alignment in all four corners with obstacle
 *      detection, e.g. a trigger at the bottom of the viewport correctly
 *      opens upward even when placed horizontally.
 *   3. If neither horizontal side has room, fall back to VERTICAL placement
 *      (below or above the trigger, left-aligned with the trigger).
 *   4. All positions are clamped to the viewport with EDGE_MARGIN clearance.
 */
function computePosition() {
  const container = containerRef.value
  const menu = menuRef.value
  if (!container || !menu) return

  const t = container.getBoundingClientRect()
  const m = menu.getBoundingClientRect()
  const ww = window.innerWidth
  const wh = window.innerHeight
  const mw = m.width || 200
  const mh = Math.min(m.height || 100, props.maxHeight)

  // Available space on each side, after accounting for trigger gap and edge margin.
  const spaceRight = ww - t.right - TRIGGER_GAP - EDGE_MARGIN
  const spaceLeft = t.left - TRIGGER_GAP - EDGE_MARGIN
  const spaceBelow = wh - t.bottom - TRIGGER_GAP - EDGE_MARGIN
  const spaceAbove = t.top - TRIGGER_GAP - EDGE_MARGIN

  const s: CSSProperties = {
    position: 'fixed',
    overflowY: 'auto',
    zIndex: '9999',
    visibility: 'visible',
  }

  const fitsRight = spaceRight >= mw
  const fitsLeft = spaceLeft >= mw

  if (props.strategy !== 'below' && (fitsRight || fitsLeft)) {
    // ── Horizontal placement (beside the trigger) ──────────────────────────
    // Choose the side with more available space, prefer right when equal.
    if (fitsRight && spaceRight >= spaceLeft) {
      s.left = `${t.right + TRIGGER_GAP}px`
    } else {
      s.right = `${ww - t.left + TRIGGER_GAP}px`
    }

    // Vertical anchor: prefer downward; fall back to upward; best-effort last.
    if (spaceBelow >= mh) {
      s.top = `${t.bottom}px`
      s.maxHeight = `${Math.min(props.maxHeight, wh - EDGE_MARGIN - t.bottom)}px`
    } else if (spaceAbove >= mh) {
      s.bottom = `${wh - t.top}px`
      s.maxHeight = `${Math.min(props.maxHeight, t.top - EDGE_MARGIN)}px`
    } else {
      if (spaceBelow >= spaceAbove) {
        s.top = `${t.bottom}px`
        s.maxHeight = `${Math.max(wh - EDGE_MARGIN - t.bottom, 80)}px`
      } else {
        s.bottom = `${wh - t.top}px`
        s.maxHeight = `${Math.max(t.top - EDGE_MARGIN, 80)}px`
      }
    }
  } else {
    // ── Vertical placement (below or above the trigger) ────────────────────
    if (spaceBelow >= spaceAbove) {
      s.top = `${t.bottom + TRIGGER_GAP}px`
      s.maxHeight = `${Math.max(wh - EDGE_MARGIN - t.bottom - TRIGGER_GAP, 80)}px`
    } else {
      s.bottom = `${wh - t.top + TRIGGER_GAP}px`
      s.maxHeight = `${Math.max(t.top - TRIGGER_GAP - EDGE_MARGIN, 80)}px`
    }
    // Horizontal: left-align with trigger; clamp to viewport.
    const maxLeft = ww - mw - EDGE_MARGIN
    s.left = `${Math.max(EDGE_MARGIN, Math.min(t.left, maxLeft))}px`
  }

  style.value = s
}

watch(
  () => props.modelValue,
  async (open) => {
    if (open) {
      // Phase 1 — render invisible so the browser lays out natural dimensions.
      style.value = {
        position: 'fixed',
        visibility: 'hidden',
        maxHeight: `${props.maxHeight}px`,
        overflowY: 'auto',
        zIndex: '9999',
      }
      await nextTick() // wait for v-if to mount the element
      computePosition() // Phase 2 — measure & apply final position.
    }
  },
)

function toggle() {
  if (!props.modelValue) {
    document.dispatchEvent(new CustomEvent('dropdown-toggle', { detail: { exclude: containerRef.value } }))
  }
  emit('update:modelValue', !props.modelValue)
}

function onClickOutside(e: MouseEvent) {
  if (!props.modelValue) return
  const target = e.target as Node
  if (!containerRef.value?.contains(target) && !menuRef.value?.contains(target)) {
    emit('update:modelValue', false)
  }
}

function onDropdownToggle(e: Event) {
  const ev = e as CustomEvent
  if (ev.detail?.exclude !== containerRef.value && props.modelValue) {
    emit('update:modelValue', false)
  }
}

function onResize() {
  if (props.modelValue) computePosition()
}

onMounted(() => {
  document.addEventListener('click', onClickOutside, { capture: true })
  document.addEventListener('dropdown-toggle', onDropdownToggle as EventListener)
  window.addEventListener('resize', onResize)
  window.addEventListener('scroll', onResize, true)
})

onUnmounted(() => {
  document.removeEventListener('click', onClickOutside, { capture: true })
  document.removeEventListener('dropdown-toggle', onDropdownToggle as EventListener)
  window.removeEventListener('resize', onResize)
  window.removeEventListener('scroll', onResize, true)
})
</script>

<style lang="scss" scoped>
.v-dropdown-container {
  position: relative;
  display: inline-block;

  &.is-block {
    display: block;
  }
}
</style>

<style lang="scss">
.v-dropdown-portal {
  display: none;
  background-color: var(--md-sys-color-surface-container);
  border-radius: 4px;
  box-shadow: rgba(0, 0, 0, 0.2) 0px 2px 6px 0px;
  border: 1px solid rgba(0, 0, 0, 0.08);
  z-index: 1050;
  white-space: nowrap;

  &.borderless { border: 0; }

  & > * {
    white-space: nowrap;
  }

  &.is-open {
    display: block;
  }

  &::-webkit-scrollbar {
    width: 6px;
  }

  &::-webkit-scrollbar-track {
    background: transparent;
  }

  &::-webkit-scrollbar-thumb {
    background: var(--md-sys-color-outline);
    border-radius: 3px;
  }
}
</style>
