<template>
  <div class="no-data-placeholder">
    <span>{{ $t(dataKey) }}</span>
    <v-text-button v-if="showSettingsLink" :loading="mutating" @click="openSettings">{{ $t('open_access_settings') }}</v-text-button>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { noDataKey } from '@/lib/list'
import { openWebSettingsGQL, initMutation } from '@/lib/api/mutation'
import { gqlFetchPeer } from '@/lib/api/peer-client'
import { findLoginPeer } from '@/lib/device/login-peers'
import tapPhone from '@/plugins/tapphone'

/** Android permission name → phone-side Access Settings option to highlight. */
const FEATURE_BY_PERMISSION: Record<string, string> = {
  WRITE_EXTERNAL_STORAGE: 'FILES',
  WRITE_CONTACTS: 'CONTACTS',
  READ_SMS: 'SMS',
  WRITE_CALL_LOG: 'CALL_LOGS',
  CALL_PHONE: 'CALL_PHONE',
  READ_PHONE_NUMBERS: 'PHONE_NUMBER',
  QUERY_ALL_PACKAGES: 'APPS',
  NOTIFICATION_LISTENER: 'NOTIFICATIONS',
}

// withDefaults is required: an absent Boolean prop resolves to false under
// Vue's boolean casting, which wrongly flipped offline/settings-link states.
const props = withDefaults(
  defineProps<{
    loading: boolean
    permissions?: string[]
    permission?: string
    placeholderKey?: string
    feature?: string
    /** Device reachability — an offline device shows 离线 instead of feature states. */
    online?: boolean
    /** Local mode: open the settings page directly on this peer instead of via the desktop server. */
    peerId?: string
    /** Force the settings link on/off; defaults to showing it for placeholder/no_permission states. */
    showSettingsLink?: boolean
  }>(),
  {
    permissions: undefined,
    permission: undefined,
    placeholderKey: undefined,
    feature: undefined,
    peerId: undefined,
    online: true,
    showSettingsLink: undefined,
  },
)

const { t } = useI18n()
const dataKey = computed(() => props.placeholderKey || noDataKey(props.loading, props.permissions ?? [], props.permission ?? '', props.online))
const showSettingsLink = computed(() => props.showSettingsLink ?? (dataKey.value === 'no_permission' || !!props.placeholderKey))
const accessFeature = computed(() => props.feature ?? (props.permission ? FEATURE_BY_PERMISSION[props.permission] : undefined))

const { mutate, loading: mutating } = initMutation({ document: openWebSettingsGQL })

function openSettings() {
  const variables = accessFeature.value ? { feature: accessFeature.value } : undefined
  if (props.peerId) {
    const peer = findLoginPeer(props.peerId)
    if (peer) void gqlFetchPeer(peer, openWebSettingsGQL, variables).catch(() => {})
  } else {
    void mutate(variables)
  }
  tapPhone(t('check_phone'))
}
</script>

<style lang="scss" scoped>
.no-data-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  text-align: center;
  padding: 40px;
}
</style>
