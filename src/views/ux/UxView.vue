<template>
  <div class="ux-page">
    <header class="ux-header">
      <h1>UI Component Library</h1>
      <p>Base components and reusable CSS patterns. All components are globally registered with V-prefix.</p>
    </header>
    <main class="ux-main">
      <ux-buttons />
      <ux-forms />
      <ux-feedback />
      <ux-overlays />
      <ux-data-display />
      <ux-css-utilities />
      <ux-markdown-editor />
    </main>
  </div>
</template>

<script setup lang="ts">
import UxButtons from './UxButtons.vue'
import UxForms from './UxForms.vue'
import UxFeedback from './UxFeedback.vue'
import UxOverlays from './UxOverlays.vue'
import UxDataDisplay from './UxDataDisplay.vue'
import UxCssUtilities from './UxCssUtilities.vue'
import UxMarkdownEditor from './UxMarkdownEditor.vue'
</script>

<style lang="scss" scoped>
.ux-page {
  max-width: 960px;
  margin: 0 auto;
  padding: 24px 16px 80px;
  color: var(--md-sys-color-on-surface);
  background: var(--md-sys-color-surface);
  min-height: 100vh;
}

.ux-header {
  margin-bottom: 32px;

  h1 {
    font-size: 1.75rem;
    font-weight: 600;
    margin: 0 0 8px;
  }

  p {
    font-size: 0.875rem;
    color: var(--md-sys-color-on-surface-variant);
    margin: 0;
  }
}

.ux-main {
  display: flex;
  flex-direction: column;
  gap: 40px;
}

:deep(.ux-section) {
  h2 {
    font-size: 1.25rem;
    font-weight: 600;
    margin: 0 0 16px;
    padding-bottom: 8px;
    border-bottom: 2px solid var(--md-sys-color-primary);
  }

  h3 {
    font-size: 0.9rem;
    font-weight: 600;
    margin: 16px 0 8px;
    color: var(--md-sys-color-on-surface-variant);
  }
}
</style>
