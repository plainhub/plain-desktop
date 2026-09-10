<template>
  <div class="failed-list">
    <div v-for="it in visibleItems" :key="it.id" class="failed-row">
      <div class="failed-info">
        <div class="failed-name">{{ nameOf(it) }}</div>
        <div class="failed-error">{{ uploadErrorText(t, it.error) }}</div>
      </div>
      <v-icon-button v-tooltip="$t('retry')" class="failed-retry-btn" @click="$emit('retry', it)">
        <i-material-symbols:refresh-rounded />
      </v-icon-button>
    </div>
    <button v-if="remaining > 0" class="show-more" @click="showMore">
      {{ $t('load_more') }} ({{ remaining }})
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { IUploadItem } from '@/stores/temp'
import { uploadErrorText } from '@/lib/upload/errors'

const props = defineProps<{
  items: IUploadItem[]
}>()

defineEmits<{
  retry: [item: IUploadItem]
}>()

const { t } = useI18n()

const INITIAL_COUNT = 50
const STEP = 500
const visibleCount = ref(INITIAL_COUNT)

const visibleItems = computed(() => props.items.slice(0, visibleCount.value))
const remaining = computed(() => props.items.length - visibleItems.value.length)

function nameOf(it: IUploadItem): string {
  return it.relativePath || it.file.name
}

function showMore() {
  visibleCount.value += STEP
}
</script>

<style scoped lang="scss">
.failed-list {
  margin-top: 8px;
  max-height: 320px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.failed-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px;
  border-radius: 8px;
  background: var(--md-sys-color-surface-container-high);
}

.failed-info {
  min-width: 0;
  flex: 1;
}

.failed-name {
  font-size: 0.875rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.failed-error {
  font-size: 0.75rem;
  color: var(--md-sys-color-error);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.failed-retry-btn {
  flex-shrink: 0;
}

.show-more {
  align-self: center;
  margin-top: 4px;
  padding: 4px 12px;
  border: none;
  border-radius: 999px;
  background: var(--md-sys-color-surface-container-highest);
  color: var(--md-sys-color-primary);
  font-size: 0.75rem;
  cursor: pointer;

  &:hover {
    background: var(--md-sys-color-surface-variant);
  }
}
</style>
