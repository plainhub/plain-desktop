<template>
  <div ref="rootRef" class="token-field" :class="{ focused: isFocused }" @mousedown="onMouseDownRoot">
    <slot name="start" />
    <div
      ref="editableRef"
      class="editable"
      role="textbox"
      contenteditable="true"
      spellcheck="false"
      :data-placeholder="placeholder"
      @focus="onFocus"
      @blur="onBlur"
      @keydown="onKeydown"
      @paste="onPaste"
      @input="onInput"
    ></div>

    <button v-tooltip="$t('search')" class="btn-icon trailing" @click.prevent.stop="emitEnter">
      <i-material-symbols:search-rounded />
    </button>

    <div v-if="menuLevel !== 'none'" class="dropdown" @mousedown.prevent>
      <template v-if="menuLevel === 'key'">
        <button
          v-for="(it, idx) in keyItems"
          :key="it.key"
          class="dd-item"
          :class="{ active: idx === activeIndex }"
          @mouseenter="activeIndex = idx"
          @click="selectKey(it.key)"
        >
          <div class="dd-main">{{ it.label }}</div>
          <div class="dd-sub">{{ it.description }}</div>
        </button>
      </template>

      <template v-else>
        <div class="dd-header">
          <button class="dd-back" type="button" aria-label="Back" @click.stop="openKeyMenu">
            <i-material-symbols:arrow-back-rounded />
          </button>
          <div class="dd-title">
            <span class="dd-title-key">{{ selectedKeyLabel }}</span>
          </div>

          <button
            v-if="selectedKey === 'history' && valueItems.length > 0"
            class="dd-clear"
            type="button"
            @click.stop="emitHistoryClear"
          >
            {{ t('clear_list') }}
          </button>
        </div>

        <div class="dd-values">
          <div v-if="selectedKey === 'start_time'" class="dd-custom">
            <div class="dd-custom-row">
              <label class="dd-custom-label">{{ t('start_time') }}</label>
              <select v-model="customStartTimeOp" class="dd-custom-op" aria-label="Operator">
                <option value=">=">&gt;=</option>
                <option value="=">=</option>
                <option value="<=">&lt;=</option>
              </select>
              <input v-model="customStartDate" class="dd-custom-date" type="date" :aria-label="t('start_time')" />
              <button class="dd-custom-apply" type="button" :disabled="!customStartDate" @click="applyCustomStartTime">
                {{ t('apply') }}
              </button>
            </div>
            <div class="dd-custom-hint">{{ t('search_calendar_select_date') }}</div>
          </div>

          <div
            v-for="(it, idx) in valueItems"
            :key="it.key + ':' + it.value"
            class="dd-item-row"
            :class="{ active: idx === activeIndex }"
            @mouseenter="activeIndex = idx"
          >
            <button class="dd-item dd-item-main" type="button" @click="selectValue(it)">
              <div class="dd-main">{{ it.label }}</div>
              <div v-if="it.description" class="dd-sub">{{ it.description }}</div>
            </button>

            <button
              v-if="selectedKey === 'history'"
              class="dd-item-delete"
              type="button"
              :aria-label="t('delete')"
              @click.stop="emitHistoryDelete(it.value)"
            >
              ×
            </button>
          </div>
          <div v-if="valueItems.length === 0 && selectedKey !== 'start_time'" class="dd-empty">{{ t('search_no_results') }}</div>
        </div>
      </template>
    </div>

    <div class="outline"></div>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useTokenSearch } from '@/hooks/token-search'
import type { Token, TokenKey } from '@/lib/token-search-dom'

export type { TokenKey, Token }

type ValueOption = string | { value: string; label: string; description?: string }

const props = withDefaults(
  defineProps<{
    text: string
    tokens: Token[]
    placeholder?: string
    enterSubmits?: boolean
    keyOptions: TokenKey[]
    valueOptions?: Record<string, ValueOption[]>
  }>(),
  { placeholder: '', enterSubmits: false, valueOptions: () => ({}) },
)

const emit = defineEmits<{
  'update:text': [value: string]
  'update:tokens': [value: Token[]]
  focus: []
  blur: []
  enter: []
  'history:select': [value: string]
  'history:delete': [value: string]
  'history:clear': []
}>()

const { t } = useI18n()

const {
  rootRef, editableRef, isFocused, menuLevel, selectedKey, selectedKeyLabel,
  activeIndex, customStartTimeOp, customStartDate,
  keyItems, valueItems,
  openKeyMenu, selectKey, selectValue, applyCustomStartTime,
  emitHistoryDelete, emitHistoryClear, emitEnter,
  onKeydown, onInput, onPaste, onFocus, onBlur, onMouseDownRoot,
} = useTokenSearch(props, emit)

defineExpose({ focus: () => editableRef.value?.focus() })
</script>

<style scoped>
.dd-custom {
  padding: 10px;
  border-bottom: 1px solid rgba(0, 0, 0, 0.08);
}

.dd-custom-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.dd-custom-label {
  font-size: 12px;
  opacity: 0.8;
  white-space: nowrap;
}

.dd-custom-op,
.dd-custom-date {
  height: 30px;
  border: 1px solid rgba(0, 0, 0, 0.15);
  border-radius: 8px;
  padding: 0 8px;
  background: transparent;
}

.dd-custom-date {
  flex: 1;
}

.dd-custom-apply {
  height: 30px;
  padding: 0 10px;
  border-radius: 8px;
  border: 1px solid rgba(0, 0, 0, 0.15);
  background: transparent;
}

.dd-custom-apply:disabled {
  opacity: 0.5;
}

.dd-custom-hint {
  margin-top: 6px;
  font-size: 12px;
  opacity: 0.7;
}
</style>
