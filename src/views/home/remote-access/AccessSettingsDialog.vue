<template>
  <v-modal width="560px" @close="close">
    <template #headline>{{ $t('access_settings') }}</template>
    <template #content>
      <div v-if="editing && !enablePending" class="access-password-fields">
        <v-text-field v-model="password" :label="$t('new_login_password')" type="password" autocomplete="new-password" :disabled="saving" />
        <v-text-field v-model="confirmation" :label="$t('confirm_login_password')" type="password" autocomplete="new-password" :disabled="saving" @keyup.enter="save" />
      </div>
      <div v-else-if="!editing && settings" class="access-settings-summary">
        <div class="access-settings-row">
          <span>{{ $t(settings.passwordType === 'FIXED' ? 'password_login_enabled' : 'password_login_disabled') }}</span>
          <v-text-button :disabled="saving" @click="editing = true">{{ $t('change_login_password') }}</v-text-button>
        </div>
        <div class="access-settings-row">
          <span>{{ $t('require_login_confirmation') }}</span>
          <v-switch
            :key="String(saving)"
            :label="$t('require_login_confirmation')"
            :checked="settings.requireApproval"
            :disabled="saving"
            @change="saveApproval"
          />
        </div>
      </div>
      <p v-if="error" role="alert">{{ $t(error) }}</p>
    </template>
    <template #actions>
      <v-text-button v-if="editing" :disabled="saving" @click="reset">{{ $t('cancel') }}</v-text-button>
      <v-filled-button v-if="editing" :loading="saving" @click="save">{{ $t(enablePending ? 'retry' : 'save') }}</v-filled-button>
      <v-text-button v-else @click="close">{{ $t('close') }}</v-text-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { popModal } from '@/components/modal'
import { readLocalAccessSettings, writeLocalAccessSetting } from '@/lib/api/local-server'

const settings = ref<Awaited<ReturnType<typeof readLocalAccessSettings>>>()
const editing = ref(false)
const password = ref('')
const confirmation = ref('')
const enablePending = ref(false)
const saving = ref(false)
const error = ref('')

onMounted(async () => {
  try {
    settings.value = await readLocalAccessSettings()
  } catch {
    error.value = 'access_settings_load_failed'
  }
})

function reset() {
  password.value = ''
  confirmation.value = ''
  enablePending.value = false
  editing.value = false
  error.value = ''
}

function close() {
  if (saving.value) return
  reset()
  popModal()
}

async function save() {
  if (saving.value || !settings.value) return
  if (!enablePending.value) {
    if (!password.value.trim() || new TextEncoder().encode(password.value).length > 1024) {
      error.value = 'invalid_login_password'
      return
    }
    if (password.value !== confirmation.value) {
      error.value = 'login_password_mismatch'
      return
    }
  }
  saving.value = true
  error.value = ''
  try {
    if (!enablePending.value) {
      await writeLocalAccessSetting('password', password.value)
      password.value = ''
      confirmation.value = ''
      enablePending.value = settings.value.passwordType === 'NONE'
    }
    if (enablePending.value) {
      await writeLocalAccessSetting('password_type', 'FIXED')
      settings.value.passwordType = 'FIXED'
    }
    reset()
  } catch {
    error.value = enablePending.value ? 'login_password_enable_failed' : 'login_password_save_failed'
  } finally {
    saving.value = false
  }
}

async function saveApproval(enabled: boolean) {
  if (saving.value || !settings.value || enabled === settings.value.requireApproval) return
  saving.value = true
  error.value = ''
  try {
    await writeLocalAccessSetting('login_require_approval', enabled)
    settings.value.requireApproval = enabled
  } catch {
    error.value = 'login_confirmation_save_failed'
  } finally {
    saving.value = false
  }
}
</script>

<style scoped>
.access-password-fields,
.access-settings-summary {
  display: grid;
  gap: 16px;
}
.access-settings-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}
</style>
