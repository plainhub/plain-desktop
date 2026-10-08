<template>
  <div>
    <p class="setup-desc">{{ $t('ai.engine_subtitle') }}</p>

    <p class="setup-hint">{{ $t('ai.setup_hint', { size: formatSize(status.modelSize) }) }}</p>

    <div v-if="status.errorMessage" class="setup-error">{{ status.errorMessage }}</div>

    <v-filled-button class="btn-block" :loading="enableLoading" @click="enable">
      {{ $t('ai.activate_download') }}
    </v-filled-button>

    <div class="setup-divider">
      <span>{{ $t('or') }}</span>
    </div>
    <AISearchModelImport :model-dir="status.modelDir" />

    <p class="setup-source" v-html="modelSourceHtml" />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { IImageSearchStatus } from '@/lib/interfaces'
import AISearchModelImport from './AISearchModelImport.vue'
import { useImageSearchActions } from '@/hooks/ai/use-image-search-actions'

const MODEL_URL = 'https://huggingface.co/google/siglip2-base-patch16-256'

defineProps<{ status: IImageSearchStatus }>()
const { t } = useI18n()

const { enable, enableLoading } = useImageSearchActions()

const modelSourceHtml = computed(() => {
  const link = `<a href="${MODEL_URL}" target="_blank" rel="noopener">${t('ai.model_source_link')}</a>`
  return t('ai.model_source_hint', { link })
})

function formatSize(bytes: number): string {
  if (bytes >= 1e9) return (bytes / 1e9).toFixed(1) + ' GB'
  if (bytes >= 1e6) return (bytes / 1e6).toFixed(0) + ' MB'
  return Math.round(bytes / 1e3) + ' KB'
}
</script>

<style lang="scss" scoped>
.setup-desc {
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.875rem;
  line-height: 1.6;
  margin: 0 0 20px;
}
.setup-cancel {
  margin-top: 12px;
}
.setup-error {
  color: var(--md-sys-color-error);
  font-size: 0.8rem;
  margin-bottom: 12px;
}
.setup-hint {
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.8rem;
  line-height: 1.5;
  margin: 0 0 20px;
}
.setup-divider {
  display: flex;
  align-items: center;
  gap: 12px;
  margin: 16px 0;
  &::before,
  &::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--md-sys-color-outline-variant);
  }
  span {
    font-size: 0.8rem;
    color: var(--md-sys-color-on-surface-variant);
  }
}
.setup-source {
  text-align: center;
  font-size: 0.8rem;
  color: var(--md-sys-color-on-surface-variant);
  margin: 20px 0 0;
  :deep(a) {
    color: var(--md-sys-color-primary);
    text-decoration: none;
    &:hover {
      text-decoration: underline;
    }
  }
}
.setup-label {
  font-size: 0.8rem;
  color: var(--md-sys-color-on-surface);
  margin: 0 0 8px;
}
.setup-pct {
  text-align: right;
  font-size: 0.8rem;
  color: var(--md-sys-color-on-surface-variant);
  margin: 4px 0 0;
}
</style>
