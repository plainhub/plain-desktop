<template>
  <form v-show="!showConfirm" @submit.prevent="onSubmit">
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
    <v-filled-button v-if="!webAccessDisabled" :disabled="isSubmitting" :loading="isSubmitting">
      {{ $t(isSubmitting ? 'logging_in' : 'log_in') }}
    </v-filled-button>
  </form>
  <div v-show="showConfirm">
    <div class="tap-phone">
      <TouchPhone />
    </div>
    <div class="tap-phone-text">
      {{ $t('login.to_continue') }}
    </div>
    <v-outlined-button @click="onCancel">
      {{ $t('cancel') }}
    </v-outlined-button>
  </div>
</template>

<script setup lang="ts">
import type { InitResult } from '@/lib/api/init'
import { useLogin } from './login'

type InitOptions = {
  autoSubmitWhenNoPassword?: boolean
}

const props = withDefaults(defineProps<{
  redirectOnSuccess?: boolean
}>(), {
  redirectOnSuccess: true,
})

const emit = defineEmits<{
  (e: 'success'): void
  (e: 'cancel'): void
}>()

const {
  showError, webAccessDisabled, showConfirm, error, showPasswordInput,
  password, passwordError, isSubmitting, onSubmit, cancel, t, applyInitResult,
} = useLogin({
  redirectOnSuccess: props.redirectOnSuccess,
  onSuccess: async () => emit('success'),
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

defineExpose({ init })
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
</style>
