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
  // Make the container block-level (full parent width)
  fullWidth: { type: Boolean, default: false },
})

const emit = defineEmits(['update:modelValue', 'opened'])

const EDGE_MARGIN = 16 // minimum clearance from viewport edges
const TRIGGER_GAP = 8  // gap between trigger element and menu
const containerRef = ref<HTMLElement | null>(null)
const menuRef = ref<HTMLElement | null>(null)
const style = ref<CSSProperties>({ position: 'fixed', visibility: 'hidden' })

function computePosition() {
  const container = containerRef.value
  const menu = menuRef.value
  if (!container || !menu) return

  const trigger = container.getBoundingClientRect()
  const { width } = menu.getBoundingClientRect()
  const { innerWidth, innerHeight } = window
  const spaceBelow = innerHeight - trigger.bottom - TRIGGER_GAP - EDGE_MARGIN
  const spaceAbove = trigger.top - TRIGGER_GAP - EDGE_MARGIN
  const menuHeight = Math.min(menu.scrollHeight + menu.offsetHeight - menu.clientHeight, props.maxHeight)
  const openBelow = spaceBelow >= menuHeight || spaceBelow >= spaceAbove
  const left = Math.max(EDGE_MARGIN, Math.min(trigger.right - width, innerWidth - width - EDGE_MARGIN))

  style.value = {
    position: 'fixed',
    overflowY: 'auto',
    zIndex: '9999',
    visibility: 'visible',
    left: `${left}px`,
    maxHeight: `${Math.max(0, Math.min(props.maxHeight, openBelow ? spaceBelow : spaceAbove))}px`,
    ...(openBelow ? { top: `${trigger.bottom + TRIGGER_GAP}px` } : { bottom: `${innerHeight - trigger.top + TRIGGER_GAP}px` }),
  }
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
      await nextTick()
      emit('opened')
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
  box-sizing: border-box;
  width: max-content;
  max-width: calc(100vw - 32px);
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
