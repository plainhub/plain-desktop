<template>
  <div class="request-approval">
    <div class="tap-phone">
      <TouchPhone aria-hidden="true" />
    </div>
    <div role="status">{{ $t('login.to_continue') }}</div>
    <RequestVerificationCode v-if="code" :code="code" :hint="hint" />
    <slot />
    <v-outlined-button @click="$emit('cancel')">{{ $t('cancel') }}</v-outlined-button>
  </div>
</template>

<script setup lang="ts">
import RequestVerificationCode from '@/components/RequestVerificationCode.vue'

defineProps<{ code?: string | null; hint: string }>()
defineEmits<{ cancel: [] }>()
</script>

<style scoped>
.request-approval {
  text-align: center;
}
.tap-phone {
  padding-block-end: 16px;
}
.tap-phone svg {
  width: 120px;
  height: auto;
  fill: var(--md-sys-color-primary);
}
.request-approval .v-outlined-button {
  margin-top: 24px;
  width: 100%;
}
</style>
