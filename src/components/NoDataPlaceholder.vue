<template>
  <div class="no-data-placeholder">
    <span>{{ $t(dataKey) }}</span>
    <a v-if="showSettingsLink" href="#" class="open-settings-link" @click.prevent="openSettings">{{ $t('open_access_settings') }}</a>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { noDataKey } from '@/lib/list'
import { openWebSettingsGQL, initMutation } from '@/lib/api/mutation'
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

const props = defineProps<{
  loading: boolean
  permissions?: string[]
  permission?: string
  placeholderKey?: string
  feature?: string
}>()

const { t } = useI18n()
const dataKey = computed(() => props.placeholderKey || noDataKey(props.loading, props.permissions ?? [], props.permission ?? ''))
const showSettingsLink = computed(() => dataKey.value === 'no_permission' || !!props.placeholderKey)
const accessFeature = computed(() => props.feature ?? (props.permission ? FEATURE_BY_PERMISSION[props.permission] : undefined))

const { mutate } = initMutation({ document: openWebSettingsGQL })

function openSettings() {
  mutate(accessFeature.value ? { feature: accessFeature.value } : undefined)
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

.open-settings-link {
  color: var(--md-sys-color-primary);
  font-size: 0.875rem;
  text-decoration: underline;
  cursor: pointer;

  &:hover {
    opacity: 0.8;
  }
}
</style>
