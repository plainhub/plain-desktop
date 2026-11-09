<template>
  <AuthShell>
    <LoginForm ref="loginFormRef" />
  </AuthShell>
</template>
<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
import AuthShell from '@/components/AuthShell.vue'
import LoginForm from './LoginForm.vue'
import { getApiHost, setPendingLoginDevice } from '@/lib/api/api'
import { requestInit } from '@/lib/api/init'
import { DeviceType } from '@/lib/status'

const loginFormRef = ref<InstanceType<typeof LoginForm> | null>(null)

onMounted(() => {
  if (!__IS_TAURI__) setPendingLoginDevice({
    name: window.location.host,
    host: getApiHost(),
    deviceType: DeviceType.OTHER,
  })
  initializeLoginForm().catch(() => {})
})

async function initializeLoginForm() {
  await nextTick()
  if (!loginFormRef.value) {
    throw new Error('login_form_not_ready')
  }
  if (__IS_TAURI__) { await loginFormRef.value.startPairing(); return }
  const result = await requestInit()
  await loginFormRef.value.init(result)
}
</script>
