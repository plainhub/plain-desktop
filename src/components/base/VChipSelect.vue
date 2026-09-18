<template>
  <v-dropdown v-model="isOpen" strategy="below">
    <template #trigger>
      <button
        type="button"
        class="v-chip-select"
        :aria-label="ariaLabel"
        :aria-haspopup="'menu'"
        :aria-expanded="isOpen"
        :disabled="disabled"
      >
        <span class="v-chip-select__label">{{ selectedOption?.label ?? placeholder }}</span>
        <component :is="icon" v-if="icon" class="v-chip-select__icon" />
        <i-lucide:chevron-down v-else class="v-chip-select__icon" :class="{ 'is-open': isOpen }" />
      </button>
    </template>

    <div
      v-for="option in options"
      :key="option.value"
      class="dropdown-item"
      :class="{ selected: option.value === modelValue, disabled: option.disabled }"
      @click="selectOption(option)"
    >
      <span>{{ option.label }}</span>
      <i-material-symbols:check-rounded
        v-if="option.value === modelValue"
        class="dropdown-item-icon"
      />
    </div>
  </v-dropdown>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { Component } from 'vue'
import VDropdown from '@/components/base/VDropdown.vue'
import type { VSelectOption } from '@/components/base/VSelect.vue'

interface Props {
  modelValue?: string | number
  options?: VSelectOption[]
  placeholder?: string
  icon?: Component
  ariaLabel?: string
  disabled?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  modelValue: undefined,
  options: () => [],
  placeholder: '',
  icon: undefined,
  ariaLabel: '',
  disabled: false,
})

const emit = defineEmits<{
  'update:modelValue': [value: string | number]
  change: [value: string | number]
}>()

const isOpen = ref(false)

const selectedOption = computed(
  () => props.options.find((o) => o.value === props.modelValue) ?? null,
)

function selectOption(option: VSelectOption) {
  if (option.disabled) return
  emit('update:modelValue', option.value)
  emit('change', option.value)
  isOpen.value = false
}
</script>

<style lang="scss" scoped>
.v-chip-select {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 28px;
  padding: 0 12px;
  border: none;
  border-radius: 999px;
  background-color: var(--md-sys-color-surface-container-high);
  color: var(--md-sys-color-on-surface-variant);
  font-family: inherit;
  font-size: 0.72rem;
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  user-select: none;
  transition: background-color 150ms ease, color 150ms ease;

  &:hover:not(:disabled),
  &[aria-expanded='true'] {
    background-color: var(--md-sys-color-surface-container-highest);
    color: var(--md-sys-color-on-surface);
  }

  &:disabled {
    opacity: 0.38;
    cursor: default;
  }

  &__icon {
    width: 16px;
    height: 16px;
    flex-shrink: 0;

    &.is-open {
      transform: rotate(180deg);
    }
  }
}
</style>
