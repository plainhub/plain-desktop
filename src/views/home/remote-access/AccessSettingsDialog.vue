<template>
  <v-modal width="560px" @close="close">
    <template #headline>
      <span>{{ $t(editing ? editTitle : 'access_settings') }}</span>
      <v-icon-button class="access-settings-close" :aria-label="$t('close')" :disabled="saving" @click="close"><i-material-symbols:close-rounded /></v-icon-button>
    </template>
    <template #content>
      <div v-if="editing && !enablePending" class="access-password-fields">
        <v-text-field v-model="password" variant="filled" :label="$t('new_login_password')" type="password" autocomplete="new-password" :disabled="saving" />
        <v-text-field v-model="confirmation" variant="filled" :label="$t('confirm_login_password')" type="password" autocomplete="new-password" :disabled="saving" @keyup.enter="save" />
      </div>
      <div v-else-if="!editing && settings" class="access-settings-summary">
        <ul class="surface-card access-settings-card">
          <v-list-item :title="$t('use_password_login')">
            <template #start><LockIcon aria-hidden="true" /></template>
            <template #end>
              <v-switch :key="String(saving)" :label="$t('use_password_login')" :checked="settings.passwordType === 'FIXED'" :disabled="saving" @click.stop @change="togglePasswordLogin" />
            </template>
          </v-list-item>
          <v-list-item :subtitle="$t(settings.passwordConfigured ? 'login_password_saved' : 'login_password_not_saved')">
            <template #start><span aria-hidden="true" /></template>
            <template #end><v-text-button :disabled="saving" @click="editing = true">{{ $t(editTitle) }}</v-text-button></template>
          </v-list-item>
        </ul>
        <ul class="surface-card access-settings-card">
          <v-list-item class="access-confirmation" :title="$t('require_login_confirmation')" :subtitle="$t('login_confirmation_hint')">
            <template #start><ShieldIcon aria-hidden="true" /></template>
            <template #end>
              <v-switch :key="String(saving)" :label="$t('require_login_confirmation')" :checked="settings.requireApproval" :disabled="saving" @click.stop @change="saveApproval" />
            </template>
          </v-list-item>
        </ul>
      </div>
      <p v-if="error" role="alert">{{ $t(error) }}</p>
    </template>
    <template v-if="editing" #actions>
      <v-text-button :disabled="saving" @click="reset">{{ $t('cancel') }}</v-text-button>
      <v-filled-button :loading="saving" @click="save">{{ $t(enablePending ? 'retry' : 'save') }}</v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import LockIcon from '~icons/material-symbols/lock-outline'
import ShieldIcon from '~icons/material-symbols/verified-user-outline-rounded'
import { popModal } from '@/components/modal'
import { readLocalAccessSettings, writeLocalAccessSetting } from '@/lib/api/local-server'

const settings = ref<Awaited<ReturnType<typeof readLocalAccessSettings>>>()
const editing = ref(false)
const editTitle = computed(() => settings.value?.passwordConfigured ? 'change_login_password' : 'set_login_password')
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
      enablePending.value = settings.value.passwordType === 'NONE' && !settings.value.passwordConfigured
      settings.value.passwordConfigured = true
      password.value = ''
      confirmation.value = ''
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

async function togglePasswordLogin(enabled: boolean) {
  if (saving.value || !settings.value || enabled === (settings.value.passwordType === 'FIXED')) return
  error.value = ''
  if (enabled && !settings.value.passwordConfigured) {
    editing.value = true
    return
  }
  saving.value = true
  try {
    const passwordType = enabled ? 'FIXED' : 'NONE'
    await writeLocalAccessSetting('password_type', passwordType)
    settings.value.passwordType = passwordType
  } catch {
    error.value = enabled ? 'login_password_toggle_enable_failed' : 'login_password_disable_failed'
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
.access-settings-close {
  margin-inline-start: auto;
}
.access-password-fields,
.access-settings-summary {
  display: grid;
  gap: 16px;
}
.access-settings-summary {
  gap: 8px;
}
.access-confirmation :deep(.subtitle) {
  white-space: normal;
}
.access-settings-card {
  margin: 0;
  padding: 0;
}
</style>
