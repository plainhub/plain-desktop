<template>
  <div>
    <p class="setup-desc">{{ $t('ai.engine_subtitle') }}</p>

    <p class="setup-hint">{{ $t('ai.setup_hint', { size: formatSize(status.modelSize) }) }}</p>

    <div v-if="status.errorMessage" class="setup-error">{{ status.errorMessage }}</div>

    <v-filled-button class="btn-block" :loading="enableLoading" @click="enable">
      {{ $t('ai.activate_download') }} · {{ BUILT_IN_IMAGE_MODEL.name }}
    </v-filled-button>

    <div class="setup-divider">
      <span>{{ $t('or') }}</span>
    </div>
    <AISearchModelImport :model-dir="status.modelDir" />

    <i18n-t keypath="ai.model_source_hint" scope="global" tag="p" class="setup-source">
      <template #link>
        <a :href="BUILT_IN_IMAGE_MODEL.sourceUrl" target="_blank" rel="noopener noreferrer" @click="openModelSource">
          {{ $t('ai.model_source_link') }}
        </a>
      </template>
    </i18n-t>
  </div>
</template>

<script setup lang="ts">
import type { IImageSearchStatus } from '@/lib/interfaces'
import AISearchModelImport from './AISearchModelImport.vue'
import { useImageSearchActions } from '@/hooks/ai/use-image-search-actions'

import { showActionError } from '@/hooks/show-action-error'
import { openUrl } from '@/lib/browser'
import { BUILT_IN_IMAGE_MODEL } from './built-in-image-model'

defineProps<{ status: IImageSearchStatus }>()

const { enable, enableLoading } = useImageSearchActions()

function openModelSource(event: MouseEvent) {
  if (!__IS_TAURI__) return
  event.preventDefault()
  void openUrl(BUILT_IN_IMAGE_MODEL.sourceUrl).catch(showActionError)
}

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
  a {
    color: var(--md-sys-color-primary);
    text-decoration: none;
    &:hover {
      text-decoration: underline;
    }
  }
}
</style>
