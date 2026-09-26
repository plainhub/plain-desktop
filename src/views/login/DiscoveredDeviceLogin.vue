<template>
  <div>
    <div v-if="initError" class="alert alert-danger show" role="alert">{{ $t(initError) }}</div>
    <LoginForm
      v-else
      ref="loginFormRef"
      :redirect-on-success="false"
      :peer="peer"
      @success="onSuccess"
      @cancel="emit('cancel')"
    />
  </div>
</template>

<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import LoginForm from './LoginForm.vue'
import { clearPendingLoginDevice, setPendingLoginDevice, type PendingLoginDevice } from '@/lib/api/api'
import { requestInit } from '@/lib/api/init'
import { clearRemoteClientId, getRemoteClientId, setRemoteClientId } from '@/lib/device/client-id'
import { loadSelfDevice } from '@/lib/device/self-device'
import type { LoginHandshakeParams } from '@/lib/api/login-handshake'

const props = defineProps<{ device: PendingLoginDevice; pairChat?: boolean }>()
const emit = defineEmits<{ success: []; cancel: [] }>()

const loginFormRef = ref<InstanceType<typeof LoginForm> | null>(null)
const initError = ref('')
const peer = ref<LoginHandshakeParams['peer']>()
const previousClientId = getRemoteClientId()
let succeeded = false
let active = true

onMounted(async () => {
  clearRemoteClientId()
  setPendingLoginDevice(props.device)
  await nextTick()
  try {
    if (props.pairChat) {
      const self = await loadSelfDevice()
      if (!self?.id || !self.publicKey || !self.port) throw new Error('local pairing identity unavailable')
      peer.value = {
        deviceName: self.name,
        port: self.port,
        deviceType: self.deviceType,
        ips: self.ips,
        signaturePublicKey: self.publicKey,
      }
    }
    if (!active) return
    const result = await requestInit()
    if (active) await loginFormRef.value?.init(result, { autoSubmitWhenNoPassword: true })
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
