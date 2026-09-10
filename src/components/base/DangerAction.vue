<template>
  <div class="danger-action">
    <template v-if="!confirming">
      <span>{{ label }}</span>
      <v-outlined-button class="danger" @click="confirming = true">{{ $t('delete') }}</v-outlined-button>
    </template>
    <template v-else>
      <span class="danger-action-confirm-text">{{ confirmText }}</span>
      <div class="danger-action-confirm-buttons">
        <v-outlined-button @click="confirming = false">{{ $t('cancel') }}</v-outlined-button>
        <v-outlined-button class="danger" :loading="loading" @click="$emit('confirm')">{{ $t('confirm') }}</v-outlined-button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

defineProps<{ label: string; confirmText: string; loading?: boolean }>()
defineEmits<{ confirm: [] }>()

const confirming = ref(false)
</script>

<style lang="scss" scoped>
.danger-action {
  display: flex;
  justify-content: space-between;
  align-items: center;
  background: var(--md-sys-color-error-container);
  border-radius: var(--pl-shape-m);
  padding: 12px 16px;
  font-size: 0.8rem;
  color: var(--md-sys-color-on-error-container);
}

.danger-action-confirm-text {
  flex: 1;
  margin-right: 12px;
}

.danger-action-confirm-buttons {
  display: flex;
  gap: 8px;
  flex-shrink: 0;
}
</style>
