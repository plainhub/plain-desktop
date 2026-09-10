<template>
  <div class="inline-delete-confirm" @click.stop>
    <div class="inline-delete-confirm-msg">
      <template v-if="messageKey">
        {{ $t(messageKey, { name, count }) }}
      </template>
      <template v-else>
        {{ count !== undefined ? $t('confirm_to_delete', { count }) : $t('confirm_to_delete_name', { name }) }}
      </template>
    </div>
    <div class="inline-delete-confirm-actions">
      <v-outlined-button @click.stop="$emit('cancel')">{{ $t('cancel') }}</v-outlined-button>
      <v-filled-button class="danger" :loading="loading" @click.stop="$emit('confirm')">{{ $t('confirm') }}</v-filled-button>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{ name?: string; count?: number; loading?: boolean; messageKey?: string }>()
defineEmits<{ confirm: []; cancel: [] }>()
</script>

<style scoped lang="scss">
.inline-delete-confirm {
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 180px;
}

.inline-delete-confirm-msg {
  font-size: 13px;
  color: var(--md-sys-color-on-surface);
  line-height: 1.4;
  white-space: break-spaces;
  word-break: break-all;
}

.inline-delete-confirm-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}
</style>
