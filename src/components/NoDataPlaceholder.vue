<template>
  <div class="no-data-placeholder">
    <span>{{ $t(dataKey) }}</span>
    <v-text-button v-if="showSettingsLink" :loading="mutating" @click="openSettings">{{ $t('open_access_settings') }}</v-text-button>
  </div>
</template>

<script setup lang="ts">
import { showActionError } from '@/hooks/show-action-error'
import { AccessErrorCode } from '@/lib/api/access-error-code'
import { GqlError } from '@/lib/api/gql-error'


import { computed } from 'vue'
import type { AccessFeature } from '@/lib/interfaces'
import { useI18n } from 'vue-i18n'
import { noDataKey } from '@/lib/list'
import { requestAccessGQL, initMutation } from '@/lib/api/mutation'
import { gqlFetchPeer } from '@/lib/api/peer-client'
import { findLoginPeer } from '@/lib/device/login-peers'
import tapPhone from '@/plugins/tapphone'
import toast from '@/components/toaster'

// withDefaults is required: an absent Boolean prop resolves to false under
// Vue's boolean casting, which wrongly flipped offline/settings-link states.
const props = withDefaults(
  defineProps<{
    loading: boolean
    access?: AccessFeature[]
    placeholderKey?: string
    feature?: AccessFeature
    /** Device reachability — an offline device shows 离线 instead of feature states. */
    online?: boolean
    /** Local mode: open the settings page directly on this peer instead of via the desktop server. */
    peerId?: string
    /** Override the access-settings action for feature-specific placeholders. */
    showSettingsLink?: boolean
  }>(),
  {
    access: undefined,
    placeholderKey: undefined,
    feature: undefined,
    peerId: undefined,
    online: true,
    showSettingsLink: undefined,
  },
)

const { t } = useI18n()
const dataKey = computed(() => props.placeholderKey || noDataKey(props.loading, props.access ?? [], props.feature ?? '', props.online))
const showSettingsLink = computed(() => !!props.feature && (props.showSettingsLink ?? (dataKey.value === AccessErrorCode.NoPermission)))

const { mutate, loading: mutating } = initMutation({ document: requestAccessGQL }, showActionError)

async function openSettings() {
  const variables = { feature: props.feature }
  tapPhone(t('check_phone'))
  try {
    let result
    if (props.peerId) {
      const peer = findLoginPeer(props.peerId)
      if (!peer) { tapPhone(''); toast(t('offline'), 'error'); return }
      result = await gqlFetchPeer(peer, requestAccessGQL, variables)
    } else {
      result = await mutate(variables)
    }
    if (result == null) { tapPhone(''); return }
    if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
    if (result.data?.requestAccess === false) {
      tapPhone('')
      toast(t(AccessErrorCode.NoPermission), 'error')
    }
  } catch (error) {
    tapPhone('')
    toast(t(error instanceof Error ? error.message : String(error)), 'error')
  }
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
