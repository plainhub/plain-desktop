<template>
  <section class="viewer-panel">
    <div class="panel-header">
      <slot name="icon" />
      <span>{{ title }}</span>
      <span v-if="meta" class="meta">{{ meta }}</span>
      <span class="spacer" />
      <slot name="actions" />
    </div>
    <div class="panel-body">
      <slot />
    </div>
  </section>
</template>

<script setup lang="ts">
defineProps<{
  title: string
  meta?: string
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
