<template>
  <form v-show="!showConfirm" @submit.prevent="onSubmit">
    <div v-show="showError" class="alert alert-danger show" role="alert">
      <i-material-symbols:error-outline-rounded />
      <div class="body">
        {{ error ? $t(error) : '' }}
        <template v-if="!nativeApp && error === 'login.peer_identity_changed'">
          <v-text-button v-if="!confirmForget" @click="confirmForget = true">{{ $t('login.forget_identity') }}</v-text-button>
          <template v-else>
            <p>{{ $t('login.confirm_forget_identity') }}</p>
            <v-outlined-button @click="confirmForget = false">{{ $t('cancel') }}</v-outlined-button>
            <v-filled-button @click="forgetIdentity">{{ $t('confirm') }}</v-filled-button>
          </template>
        </template>
      </div>
    </div>
    <v-text-field
      v-if="showPasswordInput"
      v-model="password"
      :label="t('password')"
      type="password"
      class="form-control"
      :error="!!passwordError"
      autocomplete="current-password"
      :error-text="passwordError ? $t(passwordError) : ''"
      @keydown.enter="onSubmit"
    />
    <v-filled-button v-if="!webAccessDisabled" :disabled="isSubmitting" :loading="isSubmitting">
      {{ $t(pairing ? 'pair' : isSubmitting ? 'logging_in' : 'log_in') }}
    </v-filled-button>
  </form>
  <div v-show="showConfirm">
    <div class="tap-phone">
      <TouchPhone />
    </div>
    <div class="tap-phone-text">
      {{ $t('login.to_continue') }}
    </div>
    <div v-if="verificationCode" class="login-code">
      <code>{{ verificationCode }}</code>
      <p>{{ $t('login.compare_code') }}</p>
    </div>
    <v-outlined-button @click="onCancel">
      {{ $t('cancel') }}
    </v-outlined-button>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue'
import type { InitResult } from '@/lib/api/init'
import { useLogin } from './login'
import type { LoginHandshakeParams } from '@/lib/api/login-handshake'

type InitOptions = {
  autoSubmitWhenNoPassword?: boolean
}

const nativeApp = __IS_TAURI__
const confirmForget = ref(false)

const props = withDefaults(defineProps<{
  redirectOnSuccess?: boolean
  pairing?: boolean
  peer?: LoginHandshakeParams['peer']
}>(), {
  redirectOnSuccess: true,
  pairing: false,
  peer: undefined,
})

const emit = defineEmits<{
  (e: 'success'): void
  (e: 'cancel'): void
}>()

const {
  showError, webAccessDisabled, showConfirm, verificationCode, error, showPasswordInput,
  password, passwordError, isSubmitting, onSubmit, cancel, forgetIdentity, t, applyInitResult, initializeNative, dispose,
} = useLogin({
  redirectOnSuccess: props.redirectOnSuccess,
  onSuccess: async () => emit('success'),
  peer: () => props.peer,
})

async function init(result: InitResult, options: InitOptions = {}) {
  const authenticated = applyInitResult(result)
  if (!authenticated && options.autoSubmitWhenNoPassword && !showPasswordInput.value) {
    await onSubmit()
  }
}

function onCancel() {
  cancel()
  emit('cancel')
}

onBeforeUnmount(dispose)

defineExpose({ init, initializeNative })
</script>

<style scoped>
.login-code {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  margin: 16px 0;
  padding: 16px;
  border-radius: 16px;
  background: var(--md-sys-color-surface-container-low);
  text-align: center;
}
.login-code p {
  margin: 0;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.875rem;
  line-height: 1.5;
}
.login-code code {
  color: var(--md-sys-color-on-surface);
  font-size: clamp(1.5rem, 6vw, 2rem);
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  letter-spacing: 0.04em;
  white-space: nowrap;
}
</style>

<style scoped lang="scss">
.v-filled-button,
.v-outlined-button {
  margin-top: 24px;
  width: 100%;
}

.tap-phone {
  text-align: center;
  padding-block-end: 1rem;

  *:is(svg) {
    width: 120px;
    height: auto;
    fill: var(--md-sys-color-primary);
  }
}

.tap-phone-text {
  text-align: center;
}

.alert-danger {
  margin-block-end: 16px;
}
</style>
