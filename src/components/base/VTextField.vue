<template>
  <div class="text-field-wrapper">
    <div class="v-text-field" :class="{ 'error': error, 'focused': isFocused, 'filled': modelValue, 'is-textarea': type === 'textarea' }">
      <div class="field-container">
        <textarea
          v-if="type === 'textarea'"
          ref="textareaRef"
          :value="modelValue"
          :class="inputClass"
          :rows="rows"
          :placeholder="placeholder"
          :autocomplete="autocomplete"
          @input="handleInput"
          @click="$emit('click', $event)"
          @focus="handleFocus"
          @blur="handleBlur"
          @keyup.enter="$emit('keyup.enter', $event)"
          @paste="$emit('paste', $event)"
          @drop="$emit('drop', $event)"
          @dragenter="$emit('dragenter', $event)"
          @dragleave="$emit('dragleave', $event)"
          @keydown="$emit('keydown', $event)"
          @compositionstart="$emit('compositionstart', $event)"
          @compositionend="$emit('compositionend', $event)"
        />
        <input
          v-else
          ref="inputRef"
          :type="type"
          :value="modelValue"
          :class="inputClass"
          :placeholder="placeholder"
          :autocomplete="autocomplete"
          :autocapitalize="autocapitalize"
          :autocorrect="autocorrect"
          :spellcheck="spellcheck"
          @input="handleInput"
          @click="$emit('click', $event)"
          @focus="handleFocus"
          @blur="handleBlur"
          @keyup.enter="$emit('keyup.enter', $event)"
        />
        <label v-if="label && !placeholder" class="field-label">{{ label }}</label>
        <div v-if="$slots['leading-icon']" class="leading-icon">
          <slot name="leading-icon"></slot>
        </div>
        <div v-if="$slots['trailing-icon']" class="trailing-icon">
          <slot name="trailing-icon"></slot>
        </div>
      </div>
      <div v-if="$slots.footer" class="field-footer">
        <slot name="footer"></slot>
      </div>
      <div class="field-outline"></div>
    </div>
    <div v-if="error && errorText" class="error-text">{{ errorText }}</div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'

interface Props {
  modelValue?: string
  type?: string
  label?: string
  error?: boolean
  errorText?: string
  class?: string
  rows?: number
  placeholder?: string
  autocomplete?: string
  autocapitalize?: string
  autocorrect?: string
  spellcheck?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  modelValue: '',
  type: 'text',
  label: '',
  error: false,
  errorText: '',
  class: '',
  rows: 1,
  placeholder: '',
  autocomplete: 'off',
  autocapitalize: undefined,
  autocorrect: undefined,
  spellcheck: undefined,
})

const emit = defineEmits<{
  'update:modelValue': [value: string]
  input: [event: Event]
  click: [event: MouseEvent]
  focus: [event: FocusEvent]
  blur: [event: FocusEvent]
  'keyup.enter': [event: KeyboardEvent]
  paste: [event: ClipboardEvent]
  drop: [event: DragEvent]
  dragenter: [event: DragEvent]
  dragleave: [event: DragEvent]
  keydown: [event: KeyboardEvent]
  compositionstart: [event: CompositionEvent]
  compositionend: [event: CompositionEvent]
}>()

const inputRef = ref<HTMLInputElement>()
const textareaRef = ref<HTMLTextAreaElement>()
const isFocused = ref(false)

const inputClass = computed(() => {
  const classes = ['field-input']
  if (props.class) {
    classes.push(props.class)
  }
  return classes.join(' ')
})

const handleInput = (event: Event) => {
  const target = event.target as HTMLInputElement
  emit('update:modelValue', target.value)
  emit('input', event)
}

const handleFocus = (event: FocusEvent) => {
  isFocused.value = true
  emit('focus', event)
}

const handleBlur = (event: FocusEvent) => {
  isFocused.value = false
  emit('blur', event)
}

defineExpose({
  focus: () => {
    if (props.type === 'textarea') {
      textareaRef.value?.focus()
    } else {
      inputRef.value?.focus()
    }
  },
  blur: () => {
    if (props.type === 'textarea') {
      textareaRef.value?.blur()
    } else {
      inputRef.value?.blur()
    }
  },
  getInputElement: () => props.type === 'textarea' ? textareaRef.value : inputRef.value,
})
</script>

<style lang="scss" scoped>
.text-field-wrapper {
  width: 100%;
}

.v-text-field {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 56px;
  background: var(--outlined-field-bg);

  .field-container {
    position: relative;
    display: flex;
    align-items: center;
    height: 56px;
  }

  .field-input {
    width: 100%;
    height: 56px;
    padding: 16px;
    border: none;
    outline: none;
    background: transparent;
    font-size: 16px;
    color: var(--md-sys-color-on-surface, #1d1b20);
    z-index: 1;
    resize: none;
    font-family: inherit;

    &:focus + .field-label {
      top: 0;
      transform: translateY(-50%) scale(0.75);
      color: var(--md-sys-color-primary, #6750a4);
      background: var(--outlined-field-bg, var(--md-sys-color-surface, #fffbfe));
    }
  }

  .field-label {
    position: absolute;
    left: 12px;
    top: 50%;
    transform: translateY(-50%);
    font-size: 16px;
    color: var(--md-sys-color-on-surface-variant, #49454f);
    pointer-events: none;
    transition: all 0.15s cubic-bezier(0.4, 0, 0.2, 1);
    background: transparent;
    padding: 0 4px;
    z-index: 2;
    transform-origin: left center;
    line-height: 1;
  }

  .field-footer {
    display: flex;
    align-items: center;
  }

  .field-outline {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    border: 1px solid var(--md-sys-color-outline, #79747e);
    border-radius: 4px;
    pointer-events: none;
    transition: border-color 0.2s ease;
  }

  .leading-icon {
    position: absolute;
    left: 4px;
    z-index: 3;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .trailing-icon {
    position: absolute;
    right: 4px;
    z-index: 3;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  // Focused state
  &.focused {
    .field-outline {
      border-color: var(--md-sys-color-primary, #6750a4);
      border-width: 2px;
    }

    .field-label {
      top: 0;
      transform: translateY(-50%) scale(0.75);
      color: var(--md-sys-color-primary, #6750a4);
      background: var(--outlined-field-bg, var(--md-sys-color-surface, #fffbfe));
    }
  }

  // Filled state (has value)
  &.filled .field-label {
    top: 0;
    transform: translateY(-50%) scale(0.75);
    background: var(--outlined-field-bg, var(--md-sys-color-surface, #fffbfe));
  }

  // Error state
  &.error {
    .field-outline {
      border-color: var(--md-sys-color-error, #ba1a1a);
    }

    .field-label {
      color: var(--md-sys-color-error, #ba1a1a);
    }

    &.focused .field-label {
      color: var(--md-sys-color-error, #ba1a1a);
    }
  }

  // Hover state
  &:hover:not(.focused):not(.error) .field-outline {
    border-color: var(--md-sys-color-on-surface, #1d1b20);
  }

  // Textarea specific styles
  &.is-textarea {
    height: auto;

    .field-input {
      height: auto;
      min-height: 64px;
      padding-top: 12px;
      padding-bottom: 8px;
      line-height: 1.5;
    }

    .field-container {
      height: auto;
      align-items: flex-start;
    }

    .field-label {
      top: 16px;
      transform: translateY(0);
      
      &.focused,
      &:focus + .field-label {
        top: 0;
        transform: translateY(-50%) scale(0.75);
      }
    }

    &.focused .field-label,
    &.filled .field-label {
      top: 0;
      transform: translateY(-50%) scale(0.75);
    }
  }

  // Adjust padding when there are leading icons
  &:has(.leading-icon) .field-input {
    padding-left: 48px;
  }

  // Adjust padding when there are trailing icons
  &:has(.trailing-icon) .field-input {
    padding-right: 48px;
  }
}

.error-text {
  font-size: 12px;
  color: var(--md-sys-color-error, #ba1a1a);
  line-height: 1;
  margin-top: 4px;
  margin-left: 16px;
}

// Button styling for icons
:deep(.btn-icon) {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border: none;
  background: transparent;
  border-radius: 20px;
  cursor: pointer;
  color: var(--md-sys-color-on-surface-variant, #49454f);
  transition: background-color 0.2s ease;
  position: relative;
  overflow: hidden;

  &:hover {
    background-color: var(--md-sys-color-surface-container-highest, rgba(73, 69, 79, 0.12));
  }

  &:active {
    background-color: var(--md-sys-color-surface-container-high, rgba(73, 69, 79, 0.16));
  }

  svg {
    width: 24px;
    height: 24px;
  }
}
</style>
