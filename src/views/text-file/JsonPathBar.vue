<template>
  <div class="json-path-bar">
    <label for="jsonpath-input">JSONPath</label>
    <div class="input-wrap">
      <input
        id="jsonpath-input"
        class="mono"
        :value="expression"
        :placeholder="'$.apps[*].packageName'"
        autocomplete="off"
        spellcheck="false"
        @input="onInput"
        @keydown="onKeydown"
        @focus="dropdownOpen = true"
        @blur="onBlur"
      >
      <div v-if="dropdownOpen && suggestions.length" class="suggestions">
        <button
          v-for="(item, i) in suggestions"
          :key="item.path + i"
          type="button"
          class="suggestion"
          :class="{ selected: i === selectedIndex }"
          @mousedown.prevent
          @click="apply(item.path)"
        >
          <span class="sug-type">{{ item.type }}</span>
          <span class="sug-path mono">{{ item.path }}</span>
        </button>
      </div>
    </div>
    <span class="match-chip">
      <template v-if="matchCount === -1">{{ $t('invalid_json_expr') }}</template>
      <template v-else-if="matchCount !== null">
        <b>{{ matchCount }}</b> {{ $t('n_matches', matchCount) }}
      </template>
    </span>
    <button class="icon-btn" :aria-label="$t('close')" @click="$emit('close')">
      <i-lucide-x />
    </button>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import type { PathSuggestion } from '@/lib/jsonpath'

const props = defineProps<{
  expression: string
  matchCount: number | null
  suggestions: PathSuggestion[]
}>()

const emit = defineEmits<{
  'update:expression': [value: string]
  close: []
}>()

const dropdownOpen = ref(false)
const selectedIndex = ref(0)

function onInput(e: Event) {
  emit('update:expression', (e.target as HTMLInputElement).value)
  dropdownOpen.value = true
  selectedIndex.value = 0
}

function apply(path: string) {
  emit('update:expression', path)
  dropdownOpen.value = false
}

function onBlur() {
  setTimeout(() => {
    dropdownOpen.value = false
  }, 150)
}

function onKeydown(e: KeyboardEvent) {
  if (!dropdownOpen.value || !props.suggestions.length) return
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    selectedIndex.value = Math.min(selectedIndex.value + 1, props.suggestions.length - 1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0)
  } else if (e.key === 'Enter') {
    e.preventDefault()
    const item = props.suggestions[selectedIndex.value]
    if (item) apply(item.path)
  } else if (e.key === 'Escape') {
    dropdownOpen.value = false
  }
}
</script>

<style lang="scss" scoped>
.json-path-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 40px;
  padding: 4px 12px;
  border-bottom: 1px solid var(--md-sys-color-outline-variant);
  background: var(--md-sys-color-surface-container-low);
}

label {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--md-sys-color-on-surface-variant);
  flex-shrink: 0;
}

.input-wrap {
  flex: 1;
  min-width: 0;
  position: relative;
}

input {
  width: 100%;
  height: 30px;
  padding: 0 10px;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 8px;
  background: var(--md-sys-color-surface-container-lowest);
  color: var(--md-sys-color-on-surface);
  font-family: ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, monospace;
  font-size: 0.8125rem;
  outline: none;

  &:focus {
    border-color: var(--md-sys-color-primary);
    box-shadow: 0 0 0 1px var(--md-sys-color-primary);
  }
}

.suggestions {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: 20;
  max-height: 192px;
  overflow-y: auto;
  background: var(--md-sys-color-surface-container-lowest);
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 8px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
}

.suggestion {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 12px;
  border: none;
  background: transparent;
  text-align: left;
  cursor: pointer;

  &:hover,
  &.selected {
    background: var(--md-sys-color-surface-container-high);
  }

  .sug-type {
    flex-shrink: 0;
    font-size: 0.6875rem;
    color: var(--md-sys-color-outline);
    min-width: 48px;
  }

  .sug-path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.75rem;
    color: var(--md-sys-color-on-surface);
  }
}

.match-chip {
  font-size: 0.75rem;
  color: var(--md-sys-color-on-surface-variant);
  flex-shrink: 0;
  white-space: nowrap;

  b {
    color: var(--md-sys-color-primary);
  }
}

.icon-btn {
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  flex-shrink: 0;

  &:hover {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }

  svg {
    width: 16px;
    height: 16px;
  }
}
</style>
