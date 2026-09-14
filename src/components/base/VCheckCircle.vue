<template>
  <span class="check-circle" :class="{ checked }">
    <svg v-if="checked" viewBox="0 0 24 24">
      <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z" fill="currentColor" stroke="currentColor" stroke-width="1" />
    </svg>
  </span>
</template>

<script setup lang="ts">
withDefaults(defineProps<{ checked?: boolean }>(), { checked: false })
</script>

<style scoped lang="scss">
.check-circle {
  box-sizing: border-box;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: 2px solid var(--md-sys-color-primary);
  background: var(--md-sys-color-primary-container);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition: background-color 150ms, border-color 150ms;

  &.checked {
    border-color: var(--md-sys-color-primary);
    background: var(--md-sys-color-primary);
  }

  svg {
    width: 14px;
    height: 14px;
    color: var(--md-sys-color-on-primary);
  }

  .dark &:not(.checked) {
    border-color: var(--md-sys-color-primary-container);
    background: var(--md-sys-color-on-primary-container);
  }
}
</style>
