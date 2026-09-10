<template>
  <v-modal @close="close">
    <template #headline>
      {{ $t('screen_capture_permission_title') }}
    </template>
    <template #content>
      <div class="capture-permission-guide">
        <div class="guide-icon" aria-hidden="true">
          <i-material-symbols:content-cut-rounded />
        </div>
        <p class="guide-text">{{ $t('screen_capture_permission_description') }}</p>
        <ol class="guide-steps">
          <li>{{ $t('screen_capture_permission_step_1') }}</li>
          <li>{{ $t('screen_capture_permission_step_2') }}</li>
          <li>{{ $t('screen_capture_permission_step_3') }}</li>
        </ol>
      </div>
    </template>
    <template #actions>
      <v-outlined-button @click="close">
        {{ $t('cancel') }}
      </v-outlined-button>
      <v-filled-button
        data-testid="grant-screen-capture-permission"
        :disabled="permissionActionPending"
        @click="performPermissionAction"
      >
        {{ permissionRequestAttempted ? $t('notification_open_settings') : $t('grant_permission') }}
      </v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { popModal } from '@/components/modal'
import toast from '@/components/toaster'
import {
  openScreenCapturePermissionSettings,
  requestScreenCapturePermission,
} from '@/lib/screen-capture/capture-permission'
import { reportTauriCaptureError } from '@/lib/screen-capture/tauri-capture-adapter'

const props = defineProps<{
  requestPermission?: () => Promise<boolean>
  openPermissionSettings?: () => Promise<void>
}>()
const { t } = useI18n()
const permissionActionPending = ref(false)
const permissionRequestAttempted = ref(false)

function close() {
  void popModal()
}

async function performPermissionAction() {
  if (permissionActionPending.value) return
  permissionActionPending.value = true
  const openingSettings = permissionRequestAttempted.value
  try {
    if (openingSettings) {
      await (props.openPermissionSettings ?? openScreenCapturePermissionSettings)()
      return
    }
    const granted = await (props.requestPermission ?? requestScreenCapturePermission)()
    if (granted) {
      close()
      return
    }
    permissionRequestAttempted.value = true
  } catch (error) {
    if (!openingSettings) permissionRequestAttempted.value = true
    const action = openingSettings
      ? 'opening macOS screen capture settings'
      : 'requesting macOS screen capture permission'
    void reportTauriCaptureError(`${action} failed`, error)
    toast(
      t(
        openingSettings
          ? 'screen_capture_permission_settings_failed'
          : 'screen_capture_permission_request_failed',
      ),
      'error',
    )
  } finally {
    permissionActionPending.value = false
  }
}
</script>

<style lang="scss" scoped>
.capture-permission-guide {
  max-width: 440px;
  padding: 0 8px;
}

.guide-icon {
  margin-bottom: 16px;
  text-align: center;

  i,
  svg {
    width: 64px;
    height: 64px;
    color: var(--md-sys-color-primary);
  }
}

.guide-text {
  margin: 0 0 16px;
  font-size: 0.95rem;
  line-height: 1.6;
}

.guide-steps {
  margin: 0;
  padding-left: 20px;

  li {
    color: var(--md-sys-color-on-surface-variant);
    font-size: 0.9rem;
    line-height: 1.8;
  }
}
</style>
