<template>
  <button class="v-text-button" :disabled="disabled || loading">
    <v-circular-progress v-if="loading" indeterminate class="sm" />
    <slot name="icon"></slot>
    <slot></slot>
  </button>
</template>

<script setup lang="ts">
defineProps({
  loading: { type: Boolean, default: false },
  disabled: { type: Boolean, default: false },
})
</script>

<style scoped>
.v-text-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  box-sizing: border-box;
  height: 32px;
  gap: 4px;
  border: none;
  padding: 4px 8px;
  background-color: transparent;
  color: var(--md-sys-color-primary);
  font-family: inherit;
  font-size: 0.875rem;
  font-weight: 500;
  letter-spacing: 0.1px;
  cursor: pointer;
}
.v-text-button:hover:not(:disabled) {
  opacity: 0.8;
}
.v-text-button:disabled {
  color: var(--md-sys-color-on-surface);
  opacity: 0.38;
  cursor: default;
}
</style>
