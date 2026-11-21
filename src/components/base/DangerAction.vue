<template>
  <div class="danger-action">
    <span>{{ label }}</span>
    <v-outlined-button :id="deleteAnchor" class="danger" @click="confirming = true">{{ $t('delete') }}</v-outlined-button>
    <delete-confirm-popup v-model="confirming" :anchor="deleteAnchor" :confirm-text="confirmText" :loading="loading" @confirm="$emit('confirm')" />
  </div>
</template>

<script setup lang="ts">
import { ref, useId } from 'vue'

defineProps<{ label: string; confirmText: string; loading?: boolean }>()
defineEmits<{ confirm: [] }>()

const deleteAnchor = useId()
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
</style>
