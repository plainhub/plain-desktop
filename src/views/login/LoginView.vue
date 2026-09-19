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
import { setPendingLoginDevice } from '@/lib/api/api'
import { requestInit } from '@/lib/api/init'
import { DeviceType } from '@/lib/status'

const loginFormRef = ref<InstanceType<typeof LoginForm> | null>(null)

onMounted(() => {
  setPendingLoginDevice({
    name: window.location.host,
    host: window.location.host,
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
  const result = await requestInit()
  if (result.data?.needsSetup) {
    goToSetup(); return
  }
  await loginFormRef.value.init(result)
}
</script>
