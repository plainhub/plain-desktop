<template>
  <section v-if="pairingTrust">
    <p v-if="pairingTrust.identityChanged" role="alert">{{ $t('pairing_manage_identity_changed') }}</p>
    <p v-else>{{ $t('pairing_manage_trust_hint') }}</p>
    <p v-if="showError" class="alert alert-danger show" role="alert">{{ $t(error) }}</p>
    <v-filled-button type="button" :loading="isSubmitting" @click="confirmPairingTrust">
      {{ $t('pairing_trust') }}
    </v-filled-button>
    <v-outlined-button type="button" @click="onCancel">{{ $t('cancel') }}</v-outlined-button>
  </section>
  <section v-else-if="identityChanged" class="identity-notice" role="alert">
    <div class="identity-icon"><i-material-symbols:error-outline-rounded /></div>
    <h2>{{ $t('login.PEER_IDENTITY_CHANGED') }}</h2>
    <p>{{ $t('login.identity_changed_hint') }}</p>
    <v-filled-button type="button" @click="confirmIdentityChange">{{ $t('login.confirm_identity_change') }}</v-filled-button>
  </section>
  <form v-else v-show="!showConfirm" @submit.prevent="onSubmit">
    <div v-show="showError" class="alert alert-danger show" role="alert">
      <i-material-symbols:error-outline-rounded />
      <div class="body">
        {{ error ? $t(error) : '' }}
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
    <component :is="pairing ? VOutlinedButton : VFilledButton" v-if="!webAccessDisabled" :disabled="isSubmitting" :loading="isSubmitting">
      {{ $t(pairing ? 'pair' : isSubmitting ? 'logging_in' : 'log_in') }}
    </component>
  </form>
  <div v-if="showConfirm">
    <div class="tap-phone">
      <TouchPhone aria-hidden="true" />
    </div>
    <div class="tap-phone-text" role="status">
      {{ $t('login.to_continue') }}
    </div>
    <RequestVerificationCode v-if="verificationCode" :code="verificationCode" :hint="$t('login.compare_code')" />
    <v-outlined-button @click="onCancel">
      {{ $t('cancel') }}
    </v-outlined-button>
  </div>
</template>

<script setup lang="ts">
import VOutlinedButton from '@/components/base/VOutlinedButton.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import RequestVerificationCode from '@/components/RequestVerificationCode.vue'
import { computed, onBeforeUnmount } from 'vue'
import type { InitResult } from '@/lib/api/init'
import type { SubscriptionClient } from '@/lib/api/subscription-client'
import { useLogin } from './login'

type InitOptions = {
  autoSubmitWhenNoPassword?: boolean
}

const nativeApp = __IS_TAURI__

const props = withDefaults(defineProps<{
  redirectOnSuccess?: boolean
  pairing?: boolean
  subscriptions?: SubscriptionClient
  managedDeviceId?: string
  managedOrigin?: boolean
}>(), {
  redirectOnSuccess: true,
  pairing: false,
  subscriptions: undefined,
  managedDeviceId: undefined,
})

const emit = defineEmits<{
  (e: 'success'): void
  (e: 'cancel'): void
}>()

const {
  showError, webAccessDisabled, showConfirm, pairingTrust, verificationCode, error, showPasswordInput,
  password, passwordError, isSubmitting, onSubmit, cancel, confirmPairingTrust, confirmIdentityChange, t, applyInitResult, startPairing, dispose,
} = useLogin({
  redirectOnSuccess: props.redirectOnSuccess,
  onSuccess: async () => emit('success'),
  subscriptions: props.subscriptions,
  managedDeviceId: props.managedDeviceId,
  managedOrigin: props.managedOrigin,
})

const identityChanged = computed(() => !nativeApp && !props.managedDeviceId && showError.value && error.value === 'login.PEER_IDENTITY_CHANGED')

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

defineExpose({ init, startPairing })
</script>

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

.identity-notice {
  text-align: center;
}

.identity-icon {
  display: grid;
  place-items: center;
  width: 48px;
  height: 48px;
  margin: 0 auto 24px;
  border-radius: 16px;
  background: var(--md-sys-color-surface-container);
  color: var(--md-sys-color-warning);

  svg {
    width: 24px;
    height: 24px;
  }
}

.identity-notice h2 {
  margin: 0 0 16px;
  font-size: 1.25rem;
}

.identity-notice p {
  margin: 0;
  color: var(--md-sys-color-on-surface-variant);
}

.identity-notice .v-filled-button {
  margin-top: 32px;
  height: auto;
  min-height: 48px;
  padding: 8px 16px;
}
</style>
