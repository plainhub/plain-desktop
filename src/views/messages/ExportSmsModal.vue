<template>
  <v-modal @close="popModal">
    <template #headline>{{ $t('export_sms') }}</template>
    <template #content>
      <div class="export-content">
        <div class="export-format-note">{{ $t('export_format_note') }}</div>
        <div class="export-format-list">
          <div
            v-for="fmt in formats"
            :key="fmt.value"
            class="export-format-item"
            @click="selectedFormat = fmt.value"
          >
            <i-material-symbols:radio-button-checked v-if="selectedFormat === fmt.value" class="radio-icon active" />
            <i-material-symbols:radio-button-unchecked v-else class="radio-icon" />
            <div class="export-format-info">
              <span class="export-format-label">{{ $t(fmt.labelKey) }}</span>
            </div>
          </div>
        </div>
        <div v-if="exporting" class="export-progress">
          <v-circular-progress indeterminate class="sm" />
          <span class="export-progress-text">{{ progressText }}</span>
        </div>
      </div>
    </template>
    <template #actions>
      <v-outlined-button @click="popModal">{{ $t('cancel') }}</v-outlined-button>
      <v-filled-button :loading="exporting" :disabled="exporting" @click="doExport">
        {{ $t('export') }}
      </v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import type { PropType } from 'vue'
import type { ISms } from '@/lib/interfaces'
import { popModal } from '@/components/modal'
import { formats, useExportSms } from '@/hooks/export-sms'

const props = defineProps({
  items: { type: Array as PropType<ISms[]>, default: () => [] },
  query: { type: String as PropType<string | null>, default: null },
  contactName: { type: String, default: '' },
  urlTokenKey: { type: Object as PropType<Uint8Array | null>, default: null },
})

const { selectedFormat, exporting, progressText, doExport } = useExportSms(props)
</script>

<style scoped lang="scss">
.export-content {
  width: 360px;
}

.export-format-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.export-format-item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 8px 4px;
  border-radius: 8px;
  cursor: pointer;
  transition: background 0.15s;

  &:hover {
    background: var(--md-sys-color-surface-container-high);
  }

  .radio-icon {
    flex-shrink: 0;
    font-size: 1.25rem;
    margin-top: 1px;
    color: var(--md-sys-color-on-surface-variant);

    &.active {
      color: var(--md-sys-color-primary);
    }
  }
}

.export-format-label {
  font-size: 0.9375rem;
  color: var(--md-sys-color-on-surface);
}

.export-format-note {
  margin-bottom: 12px;
  font-size: 0.75rem;
  color: var(--md-sys-color-on-surface-variant);
}

.export-progress {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 16px;
  padding: 8px 4px;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.875rem;
}

.export-progress-text {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
