<template>
  <div class="emoji-text-field">
    <v-text-field
      ref="fieldRef"
      :model-value="modelValue"
      v-bind="$attrs"
      @input="onInput"
      @click="onClick"
      @focus="emit('focus', $event)"
      @blur="onBlur"
      @keyup.enter="emit('keyup.enter', $event)"
      @paste="emit('paste', $event)"
      @drop="emit('drop', $event)"
      @dragenter="emit('dragenter', $event)"
      @dragleave="emit('dragleave', $event)"
      @keydown="onKeyDown"
      @compositionstart="emit('compositionstart', $event)"
      @compositionend="emit('compositionend', $event)"
    >
      <template v-if="$slots['leading-icon']" #leading-icon>
        <slot name="leading-icon" />
      </template>
      <template v-if="$slots['trailing-icon']" #trailing-icon>
        <slot name="trailing-icon" />
      </template>
      <template v-if="$slots.footer" #footer>
        <slot name="footer" />
      </template>
    </v-text-field>
    <div
      v-if="activeShortcode && suggestions.length"
      ref="menuRef"
      class="emoji-suggestions"
      role="listbox"
      :aria-label="$t('emoji_suggestions')"
    >
      <button
        v-for="(suggestion, index) in suggestions"
        :key="suggestion.shortcode"
        type="button"
        class="emoji-suggestion"
        :class="{ active: index === activeIndex }"
        role="option"
        :aria-selected="index === activeIndex"
        @mouseenter="activeIndex = index"
        @mousedown.prevent
        @click="selectSuggestion(suggestion)"
      >
        <span class="emoji-suggestion-character">{{ suggestion.emoji }}</span>
        <span class="emoji-suggestion-shortcode">:{{ suggestion.shortcode }}:</span>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import {
  applyEmojiSuggestion,
  findActiveEmojiShortcode,
  getEmojiSuggestions,
  replaceCompletedEmojiShortcode,
  type ActiveEmojiShortcode,
  type EmojiSuggestion,
} from '@/lib/emoji-shortcodes'

defineOptions({ inheritAttrs: false })

const props = withDefaults(defineProps<{
  modelValue?: string
}>(), {
  modelValue: '',
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

interface TextFieldHandle {
  focus: () => void
  blur: () => void
  getInputElement: () => HTMLInputElement | HTMLTextAreaElement | undefined
}

const fieldRef = ref<TextFieldHandle>()
const menuRef = ref<HTMLElement>()
const activeShortcode = ref<ActiveEmojiShortcode | null>(null)
const suggestions = ref<EmojiSuggestion[]>([])
const activeIndex = ref(0)
const currentValue = ref(props.modelValue)

watch(() => props.modelValue, (value) => {
  currentValue.value = value
})

function updateSuggestions(value: string, caret: number) {
  activeShortcode.value = findActiveEmojiShortcode(value, caret)
  suggestions.value = activeShortcode.value ? getEmojiSuggestions(activeShortcode.value.query) : []
  activeIndex.value = 0
}

function restoreCaret(caret: number) {
  nextTick(() => {
    const element = fieldRef.value?.getInputElement()
    element?.focus()
    element?.setSelectionRange(caret, caret)
  })
}

function onInput(event: Event) {
  const element = event.target as HTMLInputElement | HTMLTextAreaElement
  const caret = element.selectionStart ?? element.value.length
  if ((event as InputEvent).isComposing) {
    currentValue.value = element.value
    emit('update:modelValue', element.value)
    emit('input', event)
    return
  }
  const replacement = replaceCompletedEmojiShortcode(element.value, caret)

  if (replacement) {
    currentValue.value = replacement.value
    element.value = replacement.value
    emit('update:modelValue', replacement.value)
    updateSuggestions(replacement.value, replacement.caret)
    restoreCaret(replacement.caret)
  } else {
    currentValue.value = element.value
    emit('update:modelValue', element.value)
    updateSuggestions(element.value, caret)
  }

  emit('input', event)
}

function onKeyDown(event: KeyboardEvent) {
  if (event.isComposing || !activeShortcode.value || !suggestions.value.length) {
    emit('keydown', event)
    return
  }

  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    const direction = event.key === 'ArrowDown' ? 1 : -1
    activeIndex.value = (activeIndex.value + direction + suggestions.value.length) % suggestions.value.length
    nextTick(() => menuRef.value?.children[activeIndex.value]?.scrollIntoView({ block: 'nearest' }))
    return
  }

  const hasModifier = event.shiftKey || event.ctrlKey || event.altKey || event.metaKey
  if (!hasModifier && (event.key === 'Enter' || event.key === 'Tab')) {
    event.preventDefault()
    selectSuggestion(suggestions.value[activeIndex.value])
    return
  }

  if (event.key === 'Escape') {
    event.preventDefault()
    activeShortcode.value = null
    suggestions.value = []
    return
  }

  if (event.key === 'ArrowLeft' || event.key === 'ArrowRight' || event.key === 'Home' || event.key === 'End') {
    activeShortcode.value = null
    suggestions.value = []
  }

  emit('keydown', event)
}

function onClick(event: MouseEvent) {
  const element = event.target as HTMLInputElement | HTMLTextAreaElement
  updateSuggestions(currentValue.value, element.selectionStart ?? currentValue.value.length)
  emit('click', event)
}

function selectSuggestion(suggestion: EmojiSuggestion) {
  if (!activeShortcode.value) return

  const replacement = applyEmojiSuggestion(currentValue.value, activeShortcode.value, suggestion)
  currentValue.value = replacement.value
  emit('update:modelValue', replacement.value)
  activeShortcode.value = null
  suggestions.value = []
  restoreCaret(replacement.caret)
}

function onBlur(event: FocusEvent) {
  emit('blur', event)
  window.setTimeout(() => {
    activeShortcode.value = null
    suggestions.value = []
  })
}

defineExpose({
  focus: () => fieldRef.value?.focus(),
  blur: () => fieldRef.value?.blur(),
  getInputElement: () => fieldRef.value?.getInputElement(),
})
</script>

<style scoped lang="scss">
.emoji-text-field {
  position: relative;
  width: 100%;
}

.emoji-suggestions {
  position: absolute;
  z-index: 30;
  left: 0;
  bottom: calc(100% + 6px);
  width: min(320px, 100%);
  max-height: 264px;
  overflow-y: auto;
  padding: 6px;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 10px;
  background: var(--md-sys-color-surface-container-high);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
}

.emoji-suggestion {
  display: flex;
  align-items: center;
  width: 100%;
  gap: 10px;
  padding: 7px 10px;
  border: 0;
  border-radius: 6px;
  color: var(--md-sys-color-on-surface);
  background: transparent;
  font: inherit;
  text-align: left;
  cursor: pointer;

  &:hover,
  &.active {
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
  }
}

.emoji-suggestion-character {
  width: 26px;
  font-family: "Apple Color Emoji", "Segoe UI Emoji", "Noto Color Emoji", sans-serif;
  font-size: 20px;
  line-height: 1;
  text-align: center;
}

.emoji-suggestion-shortcode {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
