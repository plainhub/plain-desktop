<template>
  <AuthShell>
    <LoginForm ref="loginFormRef" />
  </AuthShell>
</template>
<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
import router from '@/plugins/router'
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

function goToSetup() {
  router.push({ path: '/setup', query: router.currentRoute.value.query })
}

// The view owns the /init call and the flow triage: an uninitialized
// server goes to the setup page, everything else lands in the login form.
async function initializeLoginForm() {
  await nextTick()
  if (!loginFormRef.value) {
    throw new Error('login_form_not_ready')
  }
  if (__IS_TAURI__) { await loginFormRef.value.initializeNative(); return }
  const result = await requestInit()
  if (result.data?.needsSetup) {
    goToSetup(); return
  }
  await loginFormRef.value.init(result)
}
</script>
