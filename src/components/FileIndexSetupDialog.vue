<template>
  <v-modal width="480px" :background-close="!saving" @close="cancel">
    <template #headline>
      <div class="file-access-title">
        <i-lucide:folder-lock aria-hidden="true" />
        <span>{{ $t('file_index_setup_title') }}</span>
      </div>
    </template>
    <template #content>
      <div class="file-access-content">
        <p>{{ $t('file_index_setup_body') }}</p>
        <p aria-live="polite">{{ $t(pending === 'continue' ? 'file_index_setup_waiting' : 'file_index_setup_hint') }}</p>
        <p v-if="failed" class="file-access-error" role="alert">{{ $t(failed === 'dismiss' ? 'failed' : 'file_index_setup_error') }}</p>
      </div>
    </template>
    <template #actions>
      <v-text-button :loading="pending === 'dismiss'" :disabled="saving" @click="submit('dismiss')">{{ $t('file_index_setup_dismiss') }}</v-text-button>
      <v-filled-button :loading="pending === 'continue'" :disabled="saving" @click="submit('continue')">{{ $t('file_index_setup_continue') }}</v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { closeById, modalQueue } from '@/components/modal'

const props = defineProps<{ modalId: string; onContinue: () => Promise<void>; onDismiss: () => Promise<void> }>()
const modalId = Number(props.modalId.replace(/\D/g, ''))
const pending = ref<'continue' | 'dismiss' | null>(null)
const saving = computed(() => pending.value !== null)
const failed = ref<'continue' | 'dismiss' | null>(null)

function cancel() {
  if (!saving.value) void closeById(modalId)
}

async function submit(action: 'continue' | 'dismiss') {
  pending.value = action
  failed.value = null
  try {
    await (action === 'continue' ? props.onContinue() : props.onDismiss())
    if (modalQueue.value.some((modal) => modal.id === modalId)) await closeById(modalId)
  } catch (error) {
    console.error('File index setup failed', error)
    failed.value = action
  } finally {
    pending.value = null
  }
}
</script>

<style scoped>
.file-access-title,
.file-access-content {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.file-access-title svg {
  width: 24px;
  height: 24px;
  color: var(--md-sys-color-primary);
}

.file-access-content p {
  margin: 0;
  color: var(--md-sys-color-on-surface-variant);
}

.file-access-content .file-access-error {
  color: var(--md-sys-color-error);
}
</style>
