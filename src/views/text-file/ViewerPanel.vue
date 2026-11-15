<template>
  <section class="viewer-panel">
    <div class="panel-header">
      <slot name="icon" />
      <span>{{ title }}</span>
      <span v-if="meta" class="meta">{{ meta }}</span>
      <span class="spacer" />
      <button
        v-if="wrap !== undefined"
        class="panel-button"
        :class="{ active: wrap }"
        :title="$t('wrap')"
        :aria-label="$t('wrap')"
        :aria-pressed="wrap"
        @click="$emit('update:wrap', !wrap)"
      >
        <i-lucide-wrap-text />
      </button>
      <slot name="actions" />
      <button
        class="panel-button"
        :title="fullscreen ? $t('exit_fullscreen') : $t('fullscreen')"
        :aria-label="fullscreen ? $t('exit_fullscreen') : $t('fullscreen')"
        @click="$emit('update:fullscreen', !fullscreen)"
      >
        <i-lucide-minimize v-if="fullscreen" />
        <i-lucide-maximize v-else />
      </button>
    </div>
    <div class="panel-body">
      <slot />
    </div>
  </section>
</template>

<script setup lang="ts">
withDefaults(defineProps<{
  title: string
  meta?: string
  wrap?: boolean
  fullscreen: boolean
}>(), { wrap: undefined })

defineEmits<{
  'update:wrap': [wrap: boolean]
  'update:fullscreen': [fullscreen: boolean]
}>()
</script>

<style lang="scss" scoped>
.viewer-panel {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--md-sys-color-surface-container-lowest);
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 12px;
  overflow: hidden;
}

.panel-header {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  background: var(--md-sys-color-surface-container-low);
  border-bottom: 1px solid var(--md-sys-color-outline-variant);
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--md-sys-color-on-surface-variant);
  flex-shrink: 0;

  :deep(svg) {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
  }
}

.panel-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: inherit;
  cursor: pointer;
  flex-shrink: 0;

  &:hover {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }

  &.active {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }
}

.meta {
  font-weight: 400;
  color: var(--md-sys-color-outline);
}

.spacer {
  flex: 1;
}

.panel-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  // The shell owns scrolling for contents that don't self-scroll (the
  // markdown preview div); self-scrolling children (CodeMirror, the
  // virtualized json tree) are height:100% and never overflow it.
  overflow: auto;
}
</style>
