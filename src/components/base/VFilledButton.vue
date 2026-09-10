<template>
  <button class="v-filled-button" :disabled="disabled || loading">
    <v-circular-progress v-if="loading" indeterminate class="sm" />
    <slot />
  </button>
</template>

<script setup lang="ts">
defineProps({
  loading: { type: Boolean, default: false },
  disabled: { type: Boolean, default: false }
})
</script>

<style scoped>
.v-filled-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  box-sizing: border-box;
  border: none;
  border-radius: 16px;
  padding: 4px 16px;
  background-color: var(--md-sys-color-primary, #6750A4);
  color: var(--md-sys-color-on-primary, #FFFFFF);
  font-family: inherit;
  font-size: 14px;
  font-weight: 500;
  letter-spacing: 0.1px;
  text-decoration: none;
  cursor: pointer;
  transition: box-shadow 0.2s;
  height: 32px;
  min-width: 64px;
  position: relative;
  overflow: hidden;
}

.v-filled-button:hover {
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3), 0 1px 3px 1px rgba(0, 0, 0, 0.15);
}

.v-filled-button:disabled {
  background-color: rgba(0, 0, 0, 0.12);
  color: rgba(0, 0, 0, 0.38);
  cursor: default;
  box-shadow: none;
}
</style> 