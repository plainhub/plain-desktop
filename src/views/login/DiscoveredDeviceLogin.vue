<template>
  <div>
    <div v-if="initError" class="alert alert-danger show" role="alert">{{ $t(initError) }}</div>
    <LoginForm
      v-else
      ref="loginFormRef"
      :redirect-on-success="false"
      pairing
      :subscriptions="subscriptions"
      :managed-device-id="managedDeviceId"
      @success="onSuccess"
      @cancel="emit('cancel')"
    />
  </div>
</template>

<script setup lang="ts">
import type { SubscriptionClient } from '@/lib/api/subscription-client'
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import LoginForm from './LoginForm.vue'
import { clearPendingLoginDevice, setPendingLoginDevice, type PendingLoginDevice } from '@/lib/api/api'
import { clearRemoteClientId, getRemoteClientId, setRemoteClientId } from '@/lib/device/client-id'

const props = defineProps<{ device: PendingLoginDevice & { id: string }; subscriptions?: SubscriptionClient }>()
const emit = defineEmits<{ success: []; cancel: [] }>()

const loginFormRef = ref<InstanceType<typeof LoginForm> | null>(null)
const initError = ref('')
const previousClientId = getRemoteClientId()
const managedDeviceId = __IS_TAURI__ ? undefined : props.device.id
let succeeded = false
let active = true

onMounted(async () => {
  if (!managedDeviceId) {
    clearRemoteClientId()
    setPendingLoginDevice(props.device)
  }
  await nextTick()
  try {
    await loginFormRef.value?.startPairing()
  } catch {
    if (active) initError.value = 'network_error'
  }
})

onBeforeUnmount(() => {
  active = false
  clearPendingLoginDevice()
  if (!succeeded) setRemoteClientId(previousClientId)
})

function onSuccess() {
  succeeded = true
  emit('success')
}
</script>
