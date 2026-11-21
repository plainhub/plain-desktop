<template>
  <Teleport to="body">
    <div v-if="modelValue" ref="menuRef" class="v-dropdown-menu is-open" @click.stop>
      <slot></slot>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'

const props = defineProps({
  modelValue: { type: Boolean, default: false },
  anchor: { type: String, required: true },
})

const emit = defineEmits(['update:modelValue'])

const SPACING = 8
const MIN_WIDTH = 120
const MIN_HEIGHT = 80
const menuRef = ref<HTMLElement | null>(null)

function positionMenu() {
  const menuElement = menuRef.value
  if (!menuElement) return
  const anchor = document.getElementById(props.anchor)
  if (!anchor) return
  const rect = anchor.getBoundingClientRect()

  const { innerWidth, innerHeight } = window

  const menuWidth = menuElement.offsetWidth || MIN_WIDTH
  const menuHeight = menuElement.offsetHeight || MIN_HEIGHT

  let top = rect.bottom + SPACING
  let left = rect.left

  if (left + menuWidth > innerWidth) {
    left = rect.right - menuWidth
  }
  left = Math.max(SPACING, left)

  if (top + menuHeight > innerHeight) {
    top = rect.top - menuHeight - SPACING
    if (top < SPACING) {
      top = SPACING
      menuElement.style.maxHeight = `${innerHeight - 16}px`
      menuElement.style.overflowY = 'auto'
    }
  }

  menuElement.style.top = `${top}px`
  menuElement.style.left = `${left}px`

  menuElement.style.opacity = '1'
  menuElement.style.visibility = 'visible'
}

function handleOutsideClick(event: MouseEvent) {
  const menu = menuRef.value
  if (!menu) return
  const target = event.target as Node
  if (!menu.contains(target) && !document.getElementById(props.anchor)?.contains(target)) {
    emit('update:modelValue', false)
  }
}

function handleDropdownToggle(event: Event) {
  const { detail } = event as CustomEvent
  if (menuRef.value && (!detail?.exclude || detail.exclude !== document.getElementById(props.anchor))) {
    emit('update:modelValue', false)
  }
}

watch(() => props.anchor, positionMenu, { flush: 'post' })

watch(menuRef, (menu, _, onCleanup) => {
  if (!menu) return
  positionMenu()
  document.addEventListener('click', handleOutsideClick, true)
  document.addEventListener('dropdown-toggle', handleDropdownToggle)
  onCleanup(() => {
    document.removeEventListener('click', handleOutsideClick, true)
    document.removeEventListener('dropdown-toggle', handleDropdownToggle)
  })
}, { flush: 'post' })
</script>

<style lang="scss" scoped>
.v-dropdown-menu {
  display: none;
  position: fixed;
  background: var(--md-sys-color-surface-container);
  border-radius: 8px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.14), 0 8px 24px rgba(0, 0, 0, 0.08);
  border: 1px solid rgba(0, 0, 0, 0.06);
  padding: 4px;
  z-index: 1000;
  white-space: nowrap;
  min-width: 120px;
  max-width: 300px;
  opacity: 0;
  visibility: hidden;
  transition: opacity 150ms ease-out;

  &.is-open {
    display: block;
  }

  &::-webkit-scrollbar { width: 6px; }
  &::-webkit-scrollbar-track { background: transparent; }
  &::-webkit-scrollbar-thumb {
    background: var(--md-sys-color-outline);
    border-radius: 3px;
  }

  scrollbar-width: thin;
  scrollbar-color: var(--md-sys-color-outline) transparent;
}
</style>
