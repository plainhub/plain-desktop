<template>
  <details class="default-rules">
    <summary>{{ $t('file_index.defaults_title') }}</summary>
    <div class="rules-content">
      <p>{{ $t('file_index.files_hint') }}</p>
      <p>{{ $t('file_index.skip_hint') }}</p>
      <p>{{ $t('file_index.access_hint') }}</p>
      <p v-if="rules.excludedUserLibraries">{{ $t('file_index.library_hint') }}</p>
      <div>
        <h3>{{ $t('file_index.excluded_paths') }}</h3>
        <ul>
          <li v-for="path in rules.excludedDirs" :key="path">{{ path }}</li>
        </ul>
      </div>
      <div>
        <h3>{{ $t('file_index.excluded_names') }}</h3>
        <p class="names">{{ rules.excludedNames.join(', ') }}</p>
      </div>
      <div>
        <h3>{{ $t('file_index.media_only') }}</h3>
        <p>{{ $t('file_index.media_hint') }}</p>
        <p v-if="rules.excludedPackages.length" class="names">{{ rules.excludedPackages.join(', ') }}</p>
      </div>
      <p>{{ $t('file_index.rules_priority') }}</p>
    </div>
  </details>
</template>

<script setup lang="ts">
import type { FileIndexDefaults } from '@/lib/api/file-index'
defineProps<{ rules: FileIndexDefaults }>()
</script>

<style scoped>
.default-rules {
  margin-top: 8px;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.875rem;
}
summary {
  cursor: pointer;
  color: var(--md-sys-color-primary);
  padding: 8px 0;
}
.rules-content {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
h3,
p {
  margin: 0;
  font-size: inherit;
}
h3 {
  font-weight: 500;
}
ul {
  margin: 4px 0 0;
  padding-inline-start: 16px;
  overflow-wrap: anywhere;
}
.names {
  overflow-wrap: anywhere;
}
</style>
