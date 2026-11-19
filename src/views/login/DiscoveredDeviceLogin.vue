<template>
  <div>
    <div v-if="initError" class="alert alert-danger show" role="alert">{{ $t(initError) }}</div>
    <LoginForm
      v-else
      ref="loginFormRef"
      :redirect-on-success="false"
      pairing
      :subscriptions="subscriptions"
      :managed-device-id="deviceId"
      :managed-origin="managedOrigin"
      @success="emit('success')"
      @cancel="emit('cancel')"
    ><slot /></LoginForm>
  </div>
</template>

<script setup lang="ts">
import { HttpErrorCode } from '@/lib/api/http-error-code'

import type { SubscriptionClient } from '@/lib/api/subscription-client'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import LoginForm from './LoginForm.vue'

defineProps<{ deviceId: string; subscriptions?: SubscriptionClient; managedOrigin?: boolean }>()
const emit = defineEmits<{ success: []; cancel: [] }>()

const loginFormRef = ref<InstanceType<typeof LoginForm> | null>(null)
const hasCancel = computed(() => loginFormRef.value?.hasCancel ?? false)
const initError = ref('')
let active = true

onMounted(async () => {
  try {
    await loginFormRef.value?.startPairing()
  } catch {
    if (active) initError.value = HttpErrorCode.NetworkError
  }
})

onBeforeUnmount(() => {
  active = false
})

function cancel() {
  if (loginFormRef.value) loginFormRef.value.cancel()
  else emit('cancel')
}

defineExpose({ cancel, hasCancel })
</script>
