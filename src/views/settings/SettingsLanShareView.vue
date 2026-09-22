<template>
  <div class="top-app-bar">
    <div class="title">{{ t('lan_share_desc') }}</div>
  </div>
  <div class="scroll-content settings-page">
    <section v-if="shares.length > 0 || enabled" class="card border-card">
      <h5 class="card-title">{{ t('status') }}
        <v-filled-button :loading="toggling" @click="toggleEnabled">
          {{ enabled ? t('stop') : t('start') }}
        </v-filled-button>
      </h5>
      <div class="card-body">
        <div class="status-service">
          <span class="status-badge" :class="{ on: settings?.serviceActive }">{{ t('samba_service_active') }}: {{
            settings?.serviceActive ? t('yes') : t('no') }}</span>
          <span class="status-badge" :class="{ on: settings?.serviceEnabled }">{{ t('samba_service_enabled') }}: {{
            settings?.serviceEnabled ? t('yes') : t('no') }}</span>
        </div>

        <div v-if="enabled && preferredHost && shares.length" class="hint">
          <div class="share-path-tabs" style="justify-content: space-between;">
            <v-chip-set>
              <v-filter-chip
label="Windows" :selected="sharePathMode === 'windows'"
                @click="sharePathMode = 'windows'" />
              <v-filter-chip
label="macOS / Linux" :selected="sharePathMode === 'macos/linux'"
                @click="sharePathMode = 'macos/linux'" />
            </v-chip-set>
          </div>
          <div v-for="s in shares" :key="s.name" class="share-path">
            <div class="mono share-path__text">{{ formatShareAddress(s.name) }}</div>
            <v-icon-button v-tooltip="t('copy')" @click.prevent.stop="copyShareAddress(s.name)">
              <i-material-symbols:content-copy-outline-rounded />
            </v-icon-button>
          </div>
        </div>
      </div>
    </section>

    <section class="card border-card">
      <h5 class="card-title">{{ t('samba_shares') }}
        <div class="share-actions">
          <v-outlined-button value="add" @click="addShare">{{ t('add') }}</v-outlined-button>
          <v-filled-button value="save" :loading="saving" @click="save">{{ t('save') }}</v-filled-button>
        </div>
      </h5>
      <div class="card-body">
        <div v-if="shares.length === 0" class="empty">{{ t('no_data') }}</div>

        <div v-for="(s, idx) in shares" :key="idx" class="inner-card">
          <div class="card-head">
            <div class="actions">
              <v-outlined-button value="remove" @click="removeShare(idx)">{{ t('remove') }}</v-outlined-button>
            </div>
          </div>

          <v-text-field
v-model="s.name" class="form-control" :label="t('samba_share_name')" autocomplete="off"
            :error="(submitAttempted || dirty) && !!shareErrors[idx]?.name"
            :error-text="(submitAttempted || dirty) && !!shareErrors[idx]?.name ? t(shareErrors[idx]?.name) : ''" />

          <v-text-field
v-model="s.sharePath" class="form-control" :label="t('samba_share_path')"
            placeholder="/mnt/storage/share" autocomplete="off"
            :error="(submitAttempted || dirty) && !!shareErrors[idx]?.sharePath"
            :error-text="(submitAttempted || dirty) && !!shareErrors[idx]?.sharePath ? t(shareErrors[idx]?.sharePath) : ''">
            <template #trailing-icon>
              <v-icon-button v-tooltip="t('select_folder')" @click.prevent.stop="pickSharePath(idx)">
                <i-material-symbols:folder-open-rounded />
              </v-icon-button>
            </template>
          </v-text-field>

          <div class="chip-row">
            <div class="chip-label">{{ t('samba_access') }}</div>
            <v-chip-set>
              <v-filter-chip
:label="t('samba_access_anyone_write')" :selected="s.auth === 'GUEST' && !s.readOnly"
                @click="setShareMode(idx, 'GUEST', false)" />
              <v-filter-chip
:label="t('samba_access_anyone_read')" :selected="s.auth === 'GUEST' && s.readOnly"
                @click="setShareMode(idx, 'GUEST', true)" />
              <v-filter-chip
:label="t('samba_access_password_write')" :selected="s.auth === 'PASSWORD' && !s.readOnly"
                @click="setShareMode(idx, 'PASSWORD', false)" />
              <v-filter-chip
:label="t('samba_access_password_read')" :selected="s.auth === 'PASSWORD' && s.readOnly"
                @click="setShareMode(idx, 'PASSWORD', true)" />
            </v-chip-set>
          </div>
        </div>
      </div>
    </section>

    <section class="card border-card">
      <h5 class="card-title">{{ t('samba_user_password') }}
        <v-filled-button value="set-password" :loading="passwordSaving" @click="setPassword">{{ t('save') }}</v-filled-button>
      </h5>
      <div class="card-body">
        <p class="subtle">{{ t('samba_username') }}: <span class="mono">{{ settings?.username || 'nas' }}</span></p>

        <v-text-field
v-model="newPassword" class="form-control" :label="t('samba_password')" type="password"
          autocomplete="new-password" :error="(passwordSubmitAttempted || passwordDirty) && !!newPasswordError"
          :error-text="(passwordSubmitAttempted || passwordDirty) && newPasswordError ? t(newPasswordError) : ''"
          @keyup.enter="setPassword" />
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import { useSambaSettings, type SambaShare, type SambaShareAuth } from '@/hooks/samba'
import { copyTextToClipboard } from '@/lib/clipboard'
import { promptModal } from '@/components/modal'
import DirectoryPickerModal from '@/components/DirectoryPickerModal.vue'

const { t } = useI18n()
const { settings: settingsRef, saving, toggling, passwordSaving, saveSettings, setEnabled, setUserPassword } = useSambaSettings()

const enabled = ref(false)
const shares = ref<SambaShare[]>([])
const dirty = ref(false)

const submitAttempted = ref(false)
const syncing = ref(false)

const passwordDirty = ref(false)
const passwordSubmitAttempted = ref(false)

const newPassword = ref('')

type SharePathMode = 'windows' | 'macos/linux'
const sharePathMode = ref<SharePathMode>('windows')

const settings = computed(() => settingsRef.value)

/** The web UI is served by the NAS itself, so the page host IS the samba
 *  host (the old web queried deviceInfo.ips; the contract DeviceInfo has
 *  no ips/hostname fields). */
const preferredHost = computed(() => {
  const h = window.location.hostname || ''
  // Tauri shell pages have no meaningful location host.
  if (!h || h === 'localhost' || h === '127.0.0.1' || h.endsWith('.local')) return h
  return h
})

function formatShareAddress(name: string) {
  const host = preferredHost.value
  const share = String(name || '').trim()
  if (!host || !share) return ''
  if (sharePathMode.value === 'windows') {
    return `\\\\${host}\\${share}`
  }
  // macOS and most Linux file managers accept smb://
  return `smb://${host}/${share}`
}

async function copyShareAddress(name: string) {
  const text = formatShareAddress(name)
  if (!text) return
  await copyTextToClipboard(text)
}

watch(settingsRef, (s) => {
  if (!s) return
  if (dirty.value) return
  syncing.value = true
  enabled.value = !!s.enabled
  shares.value = (s.shares ?? []).map((x) => ({
    name: String(x.name || ''),
    sharePath: String(x.sharePath || ''),
    auth: x.auth,
    readOnly: !!x.readOnly,
  }))

  if (!passwordDirty.value) {
    newPassword.value = ''
    passwordSubmitAttempted.value = false
  }
  syncing.value = false
}, { immediate: true })

watch(shares, () => {
  if (syncing.value) return
  dirty.value = true
}, { deep: true })

watch(newPassword, () => {
  passwordDirty.value = true
})

function validateShareName(name: string) {
  const v = String(name || '').trim()
  if (!v) return 'valid.required'
  if (v.length > 32) return 'invalid_value'
  if (!/^[A-Za-z0-9._ -]+$/.test(v)) return 'invalid_value'
  return ''
}

function validateSharePath(path: string) {
  const v = String(path || '').trim()
  if (!v) return 'valid.required'
  if (!v.startsWith('/')) return 'invalid_value'
  if (v.includes('\u0000')) return 'invalid_value'
  return ''
}

function validatePassword(password: string) {
  const v = String(password ?? '')
  if (v.trim() === '') return 'samba_password_required'
  if (v.length > 256) return 'invalid_value'
  return ''
}

const newPasswordError = computed(() => validatePassword(newPassword.value))

const shareErrors = computed(() => {
  const counts = new Map<string, number>()
  for (const s of shares.value) {
    const key = String(s?.name || '').trim().toLowerCase()
    if (!key) continue
    counts.set(key, (counts.get(key) || 0) + 1)
  }

  return shares.value.map((s) => {
    let nameErr = validateShareName(s?.name)
    const key = String(s?.name || '').trim().toLowerCase()
    if (!nameErr && key && (counts.get(key) || 0) > 1) nameErr = 'invalid_value'

    const sharePathErr = validateSharePath(s?.sharePath)
    return {
      name: nameErr,
      sharePath: sharePathErr,
    }
  })
})

function hasShareErrors() {
  return shareErrors.value.some((e) => !!e.name || !!e.sharePath)
}

function addShare() {
  dirty.value = true
  shares.value.push({
    name: `plainnas-${shares.value.length + 1}`,
    sharePath: '',
    auth: 'GUEST',
    readOnly: true,
  })
}

function removeShare(idx: number) {
  dirty.value = true
  shares.value.splice(idx, 1)
}

function setShareMode(idx: number, auth: SambaShareAuth, readOnly: boolean) {
  dirty.value = true
  const s = shares.value[idx]
  if (!s) return
  s.auth = auth
  s.readOnly = !!readOnly
}

async function pickSharePath(idx: number) {
  try {
    const current = shares.value[idx]?.sharePath || ''
    const selected = await promptModal<string>(DirectoryPickerModal, {
      title: t('select_folder'),
      description: '',
      initialPath: current,
      modalId: 'samba-share-picker',
    })
    if (typeof selected !== 'string') return
    const v = selected.trim()
    if (!v) return
    dirty.value = true
    if (shares.value[idx]) shares.value[idx].sharePath = v
  } catch (e) {
    // ignore cancel
    void e
  }
}

async function save() {
  submitAttempted.value = true
  dirty.value = true

  if (hasShareErrors()) {
    return
  }

  const ok = await saveSettings({
    enabled: enabled.value,
    shares: shares.value,
  })
  if (!ok) return
  dirty.value = false
  submitAttempted.value = false
  toast(t('saved'))
}

async function toggleEnabled() {
  const desired = !enabled.value
  const ok = await setEnabled(desired)
  if (!ok) return
  syncing.value = true
  enabled.value = desired
  syncing.value = false
  toast(t('saved'))
}

async function setPassword() {
  passwordSubmitAttempted.value = true
  if (newPasswordError.value) {
    return
  }

  const ok = await setUserPassword(String(newPassword.value ?? ''))
  if (!ok) return
  passwordDirty.value = false
  passwordSubmitAttempted.value = false
  newPassword.value = ''
  toast(t('saved'))
}
</script>

<style scoped>
.status-service {
  display: flex;
  gap: 8px;
}

.share-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.chip-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}

.chip-label {
  color: var(--md-sys-color-on-surface-variant);
}

.empty {
  color: var(--md-sys-color-on-surface-variant);
}

.hint {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.share-path-tabs {
  display: flex;
  align-items: center;
}

.share-path {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 10px;
  border: 1px solid var(--md-sys-color-outline-variant);
  background: var(--md-sys-color-surface);
}

.share-path__text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@media (max-width: 720px) {
  .share-path__text {
    white-space: normal;
    word-break: break-all;
  }
}
</style>
