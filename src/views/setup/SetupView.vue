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
      <div v-if="deviceHost" class="setup-device">
        <i-material-symbols:dns-outline />
        <span>{{ deviceHost }}</span>
      </div>
      <v-text-field
        ref="passwordFieldRef"
        v-model="password"
        :label="$t('setup.new_password')"
        :type="showPassword ? 'text' : 'password'"
        class="form-control"
        :error="!!passwordError"
        autocomplete="new-password"
        @keydown.enter="onSubmit"
      >
        <template #trailing-icon>
          <button
            type="button"
            class="btn-icon"
            :aria-label="$t(showPassword ? 'setup.hide_password' : 'setup.show_password')"
            @click="showPassword = !showPassword"
          >
            <i-material-symbols:visibility-outline v-if="showPassword" />
            <i-material-symbols:visibility-off-outline v-else />
          </button>
        </template>
      </v-text-field>
      <v-text-field
        v-model="confirmPassword"
        :label="$t('setup.confirm_password')"
        :type="showPassword ? 'text' : 'password'"
        class="form-control"
        :error="!!passwordError"
        autocomplete="new-password"
        :error-text="passwordError ? $t(passwordError) : ''"
        @keydown.enter="onSubmit"
      />
      <v-filled-button class="submit-button" :disabled="isSubmitting" :loading="isSubmitting">
        {{ $t(isSubmitting ? 'setup.setting_password' : 'setup.set_password') }}
      </v-filled-button>
    </form>
  </div>
</template>
<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue'
import router from '@/plugins/router'
import { sha512 } from '@/lib/api/crypto'
import { setPendingLoginDevice, getPendingLoginDevice, clearPendingLoginDevice, getApiHost, getApiBaseUrl } from '@/lib/api/api'
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
const showPassword = ref(false)
const needsSetup = ref(true)
const deviceHost = ref('')
const passwordFieldRef = ref<{ focus(): void } | null>(null)
let lastInitSignaturePublicKey = ''

// The mismatch / required hints describe the pair — clear them as soon as
// either field is edited so the form never argues with what is on screen.
watch([password, confirmPassword], () => {
  if (passwordError.value) passwordError.value = ''
})

onMounted(() => {
  setPendingLoginDevice({
    name: window.location.host,
    host: window.location.host,
    deviceType: DeviceType.OTHER,
  })
  deviceHost.value = getApiHost()
  initRequest().catch(() => {})
  nextTick(() => passwordFieldRef.value?.focus())
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
    needsSetup.value = false
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

  // An uninitialized NAS stores the password via the documented REST call;
  // the handshake afterwards proves it and derives the session token.
  if (needsSetup.value) {
    const setupResp = await fetch(`${getApiBaseUrl()}/auth/setup`, {
      method: 'POST',
      headers: { 'c-id': myClientId },
      body: JSON.stringify({ password: hash }),
    })
    if (!setupResp.ok && setupResp.status !== 409) {
      showError.value = true
      error.value = 'setup.failed'
      isSubmitting.value = false
      return
    }
    needsSetup.value = false
  }

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
  margin-block-end: 12px;
  font-size: 0.875rem;
  color: var(--md-sys-color-on-surface-variant);
}

.setup-device {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  margin-block-end: 20px;
  padding: 4px 10px;
  border-radius: 12px;
  background: var(--md-sys-color-surface-container-high, rgba(73, 69, 79, 0.08));
  font-size: 0.8125rem;
  color: var(--md-sys-color-on-surface-variant);

  svg {
    width: 16px;
    height: 16px;
  }
}

.form-control {
  margin-bottom: 16px;
}

.submit-button {
  width: 100%;
  margin-top: 8px;
}
</style>
