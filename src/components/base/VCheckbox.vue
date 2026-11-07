<template>
  <div class="v-checkbox" :class="{ 'touch-wrapper': touchTarget === 'wrapper' }" @click="handleWrapperClick">
    <input
      ref="inputRef"
      class="checkbox-input"
      type="checkbox"
      :aria-label="label"
      :checked="isChecked"
      :indeterminate="indeterminate"
      @change="handleChange"
      @click="$emit('click', $event)"
    />
    <div class="checkbox-visual" :class="{ indeterminate, checked: isChecked }">
      <svg v-if="indeterminate" class="checkbox-icon" viewBox="0 0 24 24">
        <path d="M19 13H5v-4h14v2z" fill="currentColor"/>
      </svg>
      <svg v-else-if="isChecked" class="checkbox-icon" viewBox="0 0 24 24">
        <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z" fill="currentColor" stroke="currentColor" stroke-width="1"/>
      </svg>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'

interface Props {
  label?: string
  checked?: boolean
  modelValue?: boolean
  indeterminate?: boolean
  touchTarget?: 'wrapper' | 'none'
}

const props = withDefaults(defineProps<Props>(), {
  label: undefined,
  checked: false,
  modelValue: false,
  indeterminate: false,
  touchTarget: 'none'
})

const emit = defineEmits<{
  change: [event: Event]
  click: [event: MouseEvent]
  'update:modelValue': [value: boolean]
}>()

const inputRef = ref<HTMLInputElement>()

const isChecked = computed(() => props.checked || props.modelValue)

// Sync indeterminate with native input
watch(() => props.indeterminate, (val) => {
  if (inputRef.value) inputRef.value.indeterminate = val
}, { immediate: true })

function handleWrapperClick(event: MouseEvent) {
  if (props.touchTarget === 'wrapper' && event.target === event.currentTarget) {
    inputRef.value?.click()
  }
}

function handleChange(event: Event) {
  const checked = (event.target as HTMLInputElement).checked
  emit('change', event)
  emit('update:modelValue', checked)
}

// Expose checked property for compatibility
defineExpose({
  checked: () => inputRef.value?.checked || false
})
</script>

<style scoped>
.v-checkbox {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  position: relative;
  width: 40px;
  height: 40px;
  cursor: pointer;
  vertical-align: top;
}

.v-checkbox.touch-wrapper {
  border-radius: 50%;
  transition: background-color 150ms cubic-bezier(0.4, 0, 0.2, 1);
}

.v-checkbox.touch-wrapper:hover {
  background-color: rgba(103, 80, 164, 0.08);
}

.checkbox-input {
  position: absolute;
  opacity: 0;
  width: 100%;
  height: 100%;
  margin: 0;
  cursor: pointer;
  z-index: 1;
}

.checkbox-visual {
  width: 18px;
  height: 18px;
  border: 2px solid var(--md-sys-color-outline, #79747E);
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 150ms cubic-bezier(0.4, 0, 0.2, 1);
  background-color: transparent;
  color: var(--md-sys-color-on-primary, #FFFFFF);
  flex-shrink: 0;
  box-sizing: border-box;
}

.checkbox-visual.checked,
.checkbox-visual.indeterminate {
  background-color: var(--md-sys-color-primary, #6750A4);
  border-color: var(--md-sys-color-primary, #6750A4);
}

.checkbox-icon {
  width: 18px;
  height: 18px;
}

.checkbox-input:focus-visible + .checkbox-visual {
  outline: 2px solid var(--md-sys-color-primary, #6750A4);
  outline-offset: 2px;
}

.checkbox-input:disabled + .checkbox-visual {
  border-color: rgba(0, 0, 0, 0.38);
  background-color: transparent;
  opacity: 0.38;
}

.checkbox-input:disabled + .checkbox-visual.checked,
.checkbox-input:disabled + .checkbox-visual.indeterminate {
  background-color: rgba(0, 0, 0, 0.38);
  border-color: rgba(0, 0, 0, 0.38);
}

.v-checkbox:hover .checkbox-input:not(:disabled) + .checkbox-visual:not(.checked):not(.indeterminate) {
  border-color: var(--md-sys-color-primary, #6750A4);
}

.v-checkbox:active .checkbox-visual {
  transform: scale(0.95);
}

@media (prefers-color-scheme: dark) {
  .checkbox-input:disabled + .checkbox-visual {
    border-color: rgba(255, 255, 255, 0.38);
  }
  
  .checkbox-input:disabled + .checkbox-visual.checked,
  .checkbox-input:disabled + .checkbox-visual.indeterminate {
    background-color: rgba(255, 255, 255, 0.38);
    border-color: rgba(255, 255, 255, 0.38);
  }
}
</style>
