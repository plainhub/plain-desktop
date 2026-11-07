<template>
  <aside class="sidebar" :class="{ mini: isCollapsed }" :inert="isCollapsed" :style="{ width: mainStore.sidebarWidth + 'px' }">
    <div class="sidebar-body" @click="handleMenuClick">
      <slot name="body" />
    </div>
    <div v-show="!isCollapsed" class="sidebar-drag-indicator" @mousedown="resizeWidth"></div>
  </aside>
</template>

<script setup lang="ts">
import { useMainStore } from '@/stores/main'
import { useLeftSidebarResize } from '@/hooks/sidebar'
import { computed } from 'vue'

const mainStore = useMainStore()
const props = withDefaults(defineProps<{ collapsed?: boolean }>(), { collapsed: undefined })
const emit = defineEmits<{ 'update:collapsed': [value: boolean] }>()
const isCollapsed = computed(() => props.collapsed ?? mainStore.miniSidebar)

function handleMenuClick(event: MouseEvent) {
  // Auto close sidebar on mobile when clicking menu items
  if (window.innerWidth <= 768) {
    const target = event.target as Element
    // Check if clicked element is a menu item (li, nav item, etc.)
    if (target.closest('li') || target.closest('.nav-item')) {
      if (props.collapsed === undefined) mainStore.miniSidebar = true
      else emit('update:collapsed', true)
    }
  }
}

const { resizeWidth } = useLeftSidebarResize(
  160,
  () => {
    return mainStore.sidebarWidth
  },
  (width: number) => {
    mainStore.sidebarWidth = width
  }
)
</script>
