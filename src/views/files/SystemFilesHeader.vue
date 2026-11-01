<template>
  <button class="system-files-header" :class="{ expanded }" :aria-expanded="expanded" @click="$emit('toggle')">
    <i-material-symbols:expand-more-rounded v-if="expanded" />
    <i-material-symbols:chevron-right-rounded v-else />
    <span>{{ $t('system_and_apps') }} ({{ count }})</span>
  </button>
</template>

<script setup lang="ts">
defineProps<{ count: number; expanded: boolean }>()
defineEmits<{ toggle: [] }>()
</script>

<style scoped>
.system-files-header {
  display: flex;
  align-items: center;
  gap: 8px;
  width: calc(100% - 32px);
  margin: 0 16px 8px;
  padding: 16px;
  border: 0;
  border-radius: 8px;
  background: var(--md-sys-color-surface-container);
  color: var(--md-sys-color-on-surface);
  font: inherit;
  text-align: start;
  cursor: pointer;
}
.expanded {
  margin-bottom: 0;
  border-radius: 8px 8px 0 0;
}
.system-files-header:hover {
  background: var(--md-sys-color-surface-container-high);
}
</style>
