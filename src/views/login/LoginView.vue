<template>
  <header class="header">
    <header-actions :logged-in="false" />
  </header>
  <h1>PlainApp</h1>
  <div class="login-block">
    <LoginForm ref="loginFormRef" @needs-setup="goToSetup" />
  </div>
</template>
<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
import router from '@/plugins/router'
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


<style lang="scss" scoped>
.header {
  display: flex;
  justify-content: end;
  margin-top: 6px;
}

h1 {
  margin-top: 100px;
  text-align: center;
}

.login-block {
  width: 320px;
  margin: 0 auto;
  --outlined-field-bg: var(--md-sys-color-surface-variant);
  background-color: var(--md-sys-color-surface-variant);
  border-radius: var(--pl-shape-xl);
  padding-block: var(--pl-spacing-xl);
  padding: 40px;
}
</style>
