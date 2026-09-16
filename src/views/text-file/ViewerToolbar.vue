<template>
  <nav class="viewer-toolbar">
    <div v-if="structured || kind === 'md'" class="seg mode-seg">
      <button :class="{ active: mode === 'source' }" @click="$emit('update:mode', 'source')">
        <i-lucide-code />
        <span>{{ $t('viewer_source') }}</span>
      </button>
      <button :class="{ active: mode === 'split' }" @click="$emit('update:mode', 'split')">
        <i-lucide-columns-2 />
        <span>{{ $t('split_view') }}</span>
      </button>
      <button :class="{ active: mode === 'tree' }" @click="$emit('update:mode', 'tree')">
        <i-lucide-list-tree />
        <span>{{ kind === 'md' ? $t('preview') : $t('viewer_tree') }}</span>
      </button>
    </div>

    <template v-if="structured && mode !== 'tree'">
      <div class="divider" />
      <button class="tb-btn" :disabled="!canTransform" @click="$emit('format')">
        <i-lucide-code />
        <span>{{ $t('format') }}</span>
      </button>
      <button v-if="kind === 'json'" class="tb-btn" :disabled="!canTransform" @click="$emit('minify')">
        <i-lucide-minimize-2 />
        <span>{{ $t('minify') }}</span>
      </button>
      <div v-if="mode === 'source'" class="seg">
        <button :class="{ active: indentSize === 2 }" @click="$emit('update:indentSize', 2)">2</button>
        <button :class="{ active: indentSize === 4 }" @click="$emit('update:indentSize', 4)">4</button>
      </div>
    </template>

    <button v-if="kind !== 'doc' && mode !== 'tree'" class="tb-btn" :class="{ active: wrap }" @click="$emit('update:wrap', !wrap)">
      <i-lucide-wrap-text />
      <span>{{ $t('wrap') }}</span>
    </button>

    <template v-if="structured && mode !== 'source'">
      <div class="divider" />
      <span class="tb-label">{{ $t('depth') }}</span>
      <div class="seg">
        <button v-for="d in [1, 2, 3, 4]" :key="d" :class="{ active: depth === d }" @click="$emit('update:depth', d)">
          {{ d }}
        </button>
        <button :class="{ active: depth >= 999 }" @click="$emit('update:depth', 999)">
          {{ $t('all') }}
        </button>
      </div>
    </template>

    <template v-if="kind === 'json'">
      <div class="divider" />
      <button class="tb-btn" :class="{ active: pathOpen }" @click="$emit('update:pathOpen', !pathOpen)">
        <i-lucide-filter />
        <span>JSONPath</span>
      </button>
    </template>

    <span class="grow" />
    <span v-if="big" class="big-chip">
      <i-lucide-zap />
      {{ $t('large_file_virtual') }}
    </span>
    <button
      class="tb-btn"
      :title="fullscreen ? $t('exit_fullscreen') : $t('fullscreen')"
      :aria-label="fullscreen ? $t('exit_fullscreen') : $t('fullscreen')"
      @click="$emit('toggle-fullscreen')"
    >
      <i-lucide-minimize v-if="fullscreen" />
      <i-lucide-maximize v-else />
    </button>
  </nav>
</template>

<script setup lang="ts">
import type { FileKind } from './file-kind'
import type { ViewerMode } from './useViewer'

defineProps<{
  kind: FileKind
  mode: ViewerMode
  depth: number
  pathOpen: boolean
  wrap: boolean
  indentSize: number
  big: boolean
  canTransform: boolean
  structured: boolean
  fullscreen: boolean
}>()

defineEmits<{
  'update:mode': [mode: ViewerMode]
  'update:depth': [depth: number]
  'update:pathOpen': [open: boolean]
  'update:wrap': [wrap: boolean]
  'update:indentSize': [size: number]
  'toggle-fullscreen': []
  format: []
  minify: []
}>()
</script>

<style lang="scss" scoped>
.viewer-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 44px;
  padding: 4px 12px;
  border-bottom: 1px solid var(--md-sys-color-outline-variant);
  background: var(--md-sys-color-surface);
  overflow-x: auto;
  scrollbar-width: none;

  &::-webkit-scrollbar {
    display: none;
  }
}

.grow {
  flex: 1;
}

.divider {
  width: 1px;
  height: 20px;
  background: var(--md-sys-color-outline-variant);
  margin: 0 4px;
  flex-shrink: 0;
}

.tb-label {
  font-size: 0.75rem;
  color: var(--md-sys-color-on-surface-variant);
  flex-shrink: 0;
}

.seg {
  display: flex;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 8px;
  overflow: hidden;
  flex-shrink: 0;

  button {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    font-size: 0.75rem;
    font-weight: 500;
    border: none;
    border-left: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface-container-lowest);
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
    white-space: nowrap;

    &:first-child {
      border-left: none;
    }

    &:hover {
      background: var(--md-sys-color-surface-container-low);
    }

    &.active {
      background: var(--md-sys-color-primary);
      color: var(--md-sys-color-on-primary);
    }
  }
}

.tb-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.8125rem;
  font-weight: 500;
  cursor: pointer;
  flex-shrink: 0;
  white-space: nowrap;

  &:hover:not(:disabled) {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }

  &.active {
    color: var(--md-sys-color-primary);
    background: var(--md-sys-color-primary-container);
  }

  &:disabled {
    opacity: 0.4;
    cursor: default;
  }
}

.big-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 0.6875rem;
  font-weight: 600;
  color: var(--jv-ts-fg, #b45309);
  background: var(--jv-ts-bg, #fef3c7);
  padding: 2px 8px;
  border-radius: 999px;
  flex-shrink: 0;

  :root.dark & {
    --jv-ts-fg: #fcd34d;
    --jv-ts-bg: rgba(146, 64, 14, 0.32);
  }
}

@media (max-width: 768px) {
  .tb-btn span,
  .tb-label {
    display: none;
  }
}
</style>
