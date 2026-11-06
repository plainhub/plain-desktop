<template>
  <label class="feat-switch" :class="{ disabled }">
    <input type="checkbox" role="switch" :aria-label="label" :checked="checked" :disabled="disabled" @change="$emit('change', ($event.target as HTMLInputElement).checked)" />
    <span class="switch-track"></span>
  </label>
</template>

<script setup lang="ts">
defineProps<{ checked: boolean; disabled?: boolean; label: string }>()
defineEmits<{ change: [checked: boolean] }>()
</script>

<style lang="scss" scoped>
.feat-switch {
  cursor: pointer;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
}

.feat-switch.disabled {
  cursor: not-allowed;
  opacity: 0.4;
}

.feat-switch input {
  position: absolute;
  opacity: 0;
  width: 0;
  height: 0;
}

.switch-track {
  width: 40px;
  height: 24px;
  border-radius: 12px;
  background: var(--md-sys-color-surface-container-highest);
  position: relative;
  transition: background 0.25s;
}

.switch-track::after {
  content: '';
  position: absolute;
  top: 4px;
  left: 4px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--md-sys-color-outline);
  transition: transform 0.25s, background 0.25s;
}

input:checked + .switch-track {
  background: var(--md-sys-color-primary);
}

input:checked + .switch-track::after {
  transform: translateX(16px);
  background: var(--md-sys-color-on-primary);
}

input:focus-visible + .switch-track {
  outline: 2px solid var(--md-sys-color-primary);
  outline-offset: 4px;
}
</style>
