<template>
  <header class="header">
    <header-actions :logged-in="false" />
  </header>
  <h1>PlainApp</h1>
  <div class="login-block">
    <form @submit.prevent="onSubmit">
      <div v-show="showError" class="alert alert-danger show" role="alert">
        <i-material-symbols:error-outline-rounded />
        <div class="body">
          {{ error ? $t(error) : '' }}
        </div>
      </div>
      <div class="setup-hint">{{ $t('setup.hint') }}</div>
      <v-text-field
        v-model="password"
        :label="$t('setup.new_password')"
        type="password"
        class="form-control"
        :error="!!passwordError"
        autocomplete="new-password"
        :error-text="passwordError ? $t(passwordError) : ''"
        @keydown.enter="onSubmit"
      />
      <v-text-field
        v-model="confirmPassword"
        :label="$t('setup.confirm_password')"
        type="password"
        class="form-control"
        :error="!!passwordError"
        autocomplete="new-password"
        @keydown.enter="onSubmit"
      />
      <v-filled-button :disabled="isSubmitting" :loading="isSubmitting">
        {{ $t(isSubmitting ? 'setup.setting_password' : 'setup.set_password') }}
      </v-filled-button>
    </form>
  </div>
</template>
<script setup lang="ts">
import { onMounted, ref } from 'vue'
import router from '@/plugins/router'
import { sha512 } from '@/lib/api/crypto'
import { setPendingLoginDevice, getPendingLoginDevice, clearPendingLoginDevice } from '@/lib/api/api'
import { requestInit } from '@/lib/api/init'
import { findLoginPeer, saveLoginPeer } from '@/lib/device/login-peers'
import { getRemoteClientId, setRemoteClientId } from '@/lib/device/client-id'
import { performLoginHandshake } from '@/lib/api/login-handshake'
import { get as prefsGet } from '@/lib/prefs'
import { DeviceType } from '@/lib/status'

const showError = ref(false)
const error = ref('')
const password = ref('')
const confirmPassword = ref('')
const passwordError = ref('')
const isSubmitting = ref(false)
let lastInitSignaturePublicKey = ''

onMounted(() => {
  setPendingLoginDevice({
    name: window.location.host,
    host: window.location.host,
    deviceType: DeviceType.OTHER,
  })
  initRequest().catch(() => {})
})

// An already-initialized server has nothing to do on this page.
async function initRequest() {
  const result = await requestInit()
  if (result.status !== 200 || !result.data) {
    showError.value = true; error.value = 'setup.failed'; return
  }
  if (result.data.signaturePublicKey) {
    lastInitSignaturePublicKey = result.data.signaturePublicKey
  }
  if (!result.data.needsSetup) {
    goToLogin()
  }
}

function goToLogin() {
  router.push({ path: '/login', query: router.currentRoute.value.query })
}

async function onSubmit() {
  if (!password.value?.trim()) { passwordError.value = 'valid.required'; return }
  if (password.value !== confirmPassword.value) {
    passwordError.value = 'setup.password_mismatch'; return
  }
  passwordError.value = ''
  if (isSubmitting.value) return
  isSubmitting.value = true
  showError.value = false; error.value = ''

  const hash = sha512(password.value)
  const myClientId = prefsGet('client_id', '')

  try {
    const { clientId, token, signaturePublicKey } = await performLoginHandshake({
      passwordHash: hash,
      clientId: myClientId,
      storedSignaturePublicKey: findLoginPeer(getRemoteClientId())?.publicKey,
      initSignaturePublicKey: lastInitSignaturePublicKey,
    })

    const pendingLoginDevice = getPendingLoginDevice()
    const host = pendingLoginDevice?.host || window.location.host || ''
    const deviceType = pendingLoginDevice?.deviceType || DeviceType.OTHER
    if (host && clientId) {
      await saveLoginPeer({ clientId, name: '', host, token, signaturePublicKey, deviceType })
      setRemoteClientId(clientId)
      clearPendingLoginDevice()
    }
    const r = router.currentRoute.value.query['redirect']
    window.location.href = typeof r === 'string' && r.startsWith('/') && !r.startsWith('//') ? r : '/'
  } catch (e) {
    showError.value = true
    const reason = typeof e === 'string' ? e : ''
    error.value = `setup.${reason || 'failed'}`
  } finally {
    isSubmitting.value = false
  }
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

.alert-danger {
  margin-block-end: 16px;
}

.setup-hint {
  margin-block-end: 16px;
  font-size: 0.875rem;
  color: var(--md-sys-color-on-surface-variant);
}
</style>
