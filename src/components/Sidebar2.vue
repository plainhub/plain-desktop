<template>
  <aside class="sidebar2" :class="{ 'sidebar2-full': !detail }" :style="detail ? { width: mainStore.sidebar2Width + 'px' } : undefined">
    <slot />
    <div class="sidebar-drag-indicator" @mousedown="resizeWidth"></div>
  </aside>
</template>

<script setup lang="ts">
import { useMainStore } from '@/stores/main'
import { useLeftSidebarResize } from '@/hooks/sidebar'

defineProps<{ detail?: boolean }>()

const mainStore = useMainStore()
const { resizeWidth } = useLeftSidebarResize(
  300,
  () => mainStore.sidebar2Width,
  (width: number) => { mainStore.sidebar2Width = width },
)
</script>

<style scoped lang="scss">
.sidebar2 {
  position: relative;
  display: flex;
  flex-direction: column;
  height: calc(100vh - var(--pl-top-app-bar-height));

  &.sidebar2-full {
    flex: 1;
  }

  :deep(.item-link) {
    text-decoration: none;
    display: block;
  }
}
</style>
