<template>
  <v-dropdown v-model="isOpen" class="v-select" :full-width="fullWidth">
    <template #trigger>
      <button
        :class="['v-select-trigger', `v-select-trigger--${size}`, { 'v-select-trigger--full-width': fullWidth }]"
        :disabled="disabled"
        type="button"
      >
        <component :is="selectedOption?.icon" v-if="selectedOption?.icon" class="v-select-trigger__icon" />
        <span class="v-select-trigger__label">
          {{ selectedOption ? selectedOption.label : placeholder }}
        </span>
        <i-lucide:chevron-down :class="['v-select-trigger__chevron', { 'is-open': isOpen }]" />
      </button>
    </template>

    <div
      v-for="option in options"
      :key="option.value"
      class="dropdown-item"
      :class="{ selected: option.value === modelValue, disabled: option.disabled }"
      @click="selectOption(option)"
    >
      <component :is="option.icon" v-if="option.icon" class="v-select-option__icon" />
      <div v-if="option.subtitle" class="v-select-option__text">
        <span class="v-select-option__label">{{ option.label }}</span>
        <span class="v-select-option__subtitle">{{ option.subtitle }}</span>
      </div>
      <span v-else class="v-select-option__label">{{ option.label }}</span>
      <i-material-symbols:check-rounded v-if="option.value === modelValue" class="v-select-option__check" />
    </div>
  </v-dropdown>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { Component } from 'vue'

export interface VSelectOption {
  value: string | number
  label: string
  subtitle?: string
  icon?: Component
  disabled?: boolean
}

interface Props {
  modelValue?: string | number
  placeholder?: string
  disabled?: boolean
  options?: VSelectOption[]
  size?: 'sm' | 'lg'
  fullWidth?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  modelValue: undefined,
  placeholder: '',
  options: () => [],
  size: 'lg',
})

const emit = defineEmits<{
  'update:modelValue': [value: string | number]
  'change': [value: string | number]
}>()

const isOpen = ref(false)

const selectedOption = computed(() => props.options.find((o) => o.value === props.modelValue) ?? null)

function selectOption(option: VSelectOption) {
  if (option.disabled) return
  emit('update:modelValue', option.value)
  emit('change', option.value)
  isOpen.value = false
}
</script>

<style lang="scss" scoped>
.v-select {
  max-inline-size: 100%;
}

.v-select-trigger {
  display: inline-flex;
  box-sizing: border-box;
  max-inline-size: 100%;
  align-items: center;
  gap: 6px;
  border: 1px solid var(--md-sys-color-outline);
  border-radius: 20px;
  padding: 0 10px 0 14px;
  background-color: transparent;
  color: var(--md-sys-color-on-surface);
  font-family: inherit;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  min-width: 64px;
  transition: background-color 150ms cubic-bezier(0.4, 0, 0.2, 1);

  &--full-width {
    width: 100%;

    .v-select-trigger__chevron {
      margin-inline-start: auto;
    }
  }

  &--sm {
    height: 40px;
  }

  &--lg {
    height: 56px;
    border-radius: 4px;
    padding: 0 12px 0 16px;
    font-size: 1rem;
  }

  &:hover:not(:disabled) {
    background-color: rgba(0, 0, 0, 0.08);
  }

  :root.dark &:hover:not(:disabled) {
    background-color: rgba(255, 255, 255, 0.08);
  }

  &:disabled {
    border-color: rgba(0, 0, 0, 0.12);
    color: rgba(0, 0, 0, 0.38);
    cursor: default;
  }

  &__icon {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    color: var(--md-sys-color-on-surface-variant);
  }

  &__label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  &__chevron {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    color: var(--md-sys-color-on-surface-variant);
    transition: transform 0.2s ease;

    &.is-open {
      transform: rotate(180deg);
    }
  }
}
</style>

<style lang="scss">
.v-select-option {
  &__icon {
    width: 18px;
    height: 18px;
    flex-shrink: 0;
    color: var(--md-sys-color-on-surface-variant);
  }

  &__text {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 1px;
  }

  &__label {
    line-height: 1.3;
  }

  &__subtitle {
    font-size: 0.75rem;
    color: var(--md-sys-color-on-surface-variant);
    line-height: 1.3;
  }

  &__check {
    width: 18px;
    height: 18px;
    flex-shrink: 0;
    color: var(--md-sys-color-primary);
    margin-left: auto;
  }
}
</style>

<style lang="scss">
.v-select-menu {
  background: var(--md-sys-color-surface-container);
  border-radius: 4px;
  box-shadow: var(--md-sys-elevation-shadow-2);
  overflow: hidden;
  padding: 8px 0;
  max-height: inherit;
  overflow-y: auto;
}

.v-select-option {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  cursor: pointer;
  color: var(--md-sys-color-on-surface);
  transition: background-color 0.15s ease;
  
  &:hover:not(&--disabled) {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }
  
  &--selected {
    background: color-mix(in srgb, var(--md-sys-color-primary) 12%, transparent);
    color: var(--md-sys-color-primary);
  }
  
  &--disabled {
    cursor: default;
    opacity: 0.38;
    pointer-events: none;
  }
  
  &__check {
    width: 18px;
    height: 18px;
    color: var(--md-sys-color-primary);
  }
}

.dark {
  .v-select-menu {
    background: var(--md-sys-color-surface-container-high);
  }
}
</style>
