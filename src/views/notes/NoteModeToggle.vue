<template>
  <div class="toggle-group" role="group" :aria-label="$t('editor_view')">
    <button
      v-tooltip="$t('editor_view')"
      class="toggle-btn"
      :class="{ active: mode === 'editor' }"
      :aria-label="$t('editor_view')"
      :aria-pressed="mode === 'editor'"
      type="button"
      @click="$emit('update:mode', 'editor')"
    >
      <i-lucide:pencil />
    </button>
    <button
      v-tooltip="$t('markdown_source')"
      class="toggle-btn"
      :class="{ active: mode === 'source' }"
      :aria-label="$t('markdown_source')"
      :aria-pressed="mode === 'source'"
      type="button"
      @click="$emit('update:mode', 'source')"
    >
      <i-lucide:code />
    </button>
    <button
      v-tooltip="$t('preview')"
      class="toggle-btn"
      :class="{ active: mode === 'preview' }"
      :aria-label="$t('preview')"
      :aria-pressed="mode === 'preview'"
      type="button"
      @click="$emit('update:mode', 'preview')"
    >
      <i-lucide:eye />
    </button>
  </div>
</template>

<script setup lang="ts">
import type { ViewMode } from '@/hooks/note-edit'

defineProps<{ mode: ViewMode }>()
defineEmits<{ (e: 'update:mode', value: ViewMode): void }>()
</script>

<style scoped lang="scss">
.toggle-group {
  display: inline-flex;
  border-radius: 999px;
  overflow: hidden;
  background: var(--md-sys-color-surface-container-low);
}

.toggle-btn {
  border: none;
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  padding: 8px 16px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 0.82rem;
  cursor: pointer;
}

.toggle-btn.active {
  background: var(--md-sys-color-primary-container);
  color: var(--md-sys-color-on-primary-container);
}
</style>
