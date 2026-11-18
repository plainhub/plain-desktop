<template>
  <div class="quick-content-main">
    <div class="top-app-bar">
      <button v-tooltip="$t('close')" class="btn-icon" @click.prevent="store.quick = ''">
        <i-material-symbols:arrow-back-rounded />
      </button>
      <div class="title">
        {{ $t('header_actions.clipboard') }}
        <v-switch
          v-if="!peerId"
          :checked="clipboardEnabled === true"
          :disabled="savingMaster || clipboardEnabled === undefined"
          :label="$t('header_actions.clipboard')"
          defer-change
          @change="setMaster"
        />
      </div>
      <div class="actions">
        <v-chip-select
          v-if="peerId"
          :model-value="direction"
          :options="directionOptions"
          :icon="IconSync"
          :aria-label="`${$t('clipboard_sync_direction')}: ${directionLabel}`"
          :disabled="savingDirection"
          plain
          icon-only
          @update:model-value="pickDirection"
        />
        <v-chip-select
          :model-value="peerId"
          :options="devices"
          :icon="IconDevices"
          :aria-label="$t('device')"
          plain
          @update:model-value="selectPeer"
        />
      </div>
    </div>

    <div class="quick-content-body">
      <clipboard-history v-if="items.length" :items="items" @delete="removeItem" />
      <NoDataPlaceholder
        v-else
        :loading="status === 'loading'"
        :online="status !== 'offline'"
        :placeholder-key="status === 'disabled' ? 'clipboard_disabled_on_device' : status === 'error' ? 'FAILED' : ''"
      />
      <v-pagination
        v-if="total > limit"
        :page="page" :go="gotoPage" :total="total" :limit="limit"
        :page-size="limit" :on-change-page-size="changePageSize"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import IconSync from '~icons/material-symbols/sync-rounded'
import IconDevices from '~icons/lucide/monitor-smartphone'
import VChipSelect from '@/components/base/VChipSelect.vue'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import toast from '@/components/toaster'
import type { IClipboardItem } from '@/lib/interfaces'
import { isClipboardDirection, peerClipboardDirection, setPeerClipboardDirection } from '@/lib/peer/local-clipboard-data'
import { useMainStore } from '@/stores/main'
import ClipboardHistory from './ClipboardHistory.vue'
import { useLocalClipboard } from './local-clipboard'
import { useClipboardMasterSwitch } from './master-switch'

const store = useMainStore()
const { t } = useI18n()
const { peerId, clipboardPeers, items, status, total, limit, page, gotoPage, changePageSize, deleteItem } = useLocalClipboard()
const devices = computed(() => [
  { value: '', label: t('this_device') },
  ...clipboardPeers.value.map(peer => ({ value: peer.id, label: peer.name })),
])
const directionOptions = computed(() => [
  { value: 'OFF', label: t('clipboard_direction_off') },
  { value: 'SEND', label: t('clipboard_direction_send') },
  { value: 'RECEIVE', label: t('clipboard_direction_receive') },
  { value: 'BOTH', label: t('clipboard_direction_both') },
])
const direction = computed(() => peerClipboardDirection(peerId.value))
const directionLabel = computed(() => directionOptions.value.find(option => option.value === direction.value)!.label)
const { enabled: clipboardEnabled, saving: savingMaster, setEnabled } = useClipboardMasterSwitch(() => !peerId.value && store.quick === 'clipboard')

async function setMaster(checked: boolean) {
  try { await setEnabled(checked) } catch { toast(t('FAILED'), 'error') }
}
const savingDirection = ref(false)

function selectPeer(value: string | number) {
  if (typeof value === 'string' && (!value || clipboardPeers.value.some(peer => peer.id === value))) peerId.value = value
}

async function pickDirection(value: string | number) {
  if (!peerId.value || !isClipboardDirection(value) || value === direction.value || savingDirection.value) return
  savingDirection.value = true
  try {
    await setPeerClipboardDirection(peerId.value, value)
  } catch {
    toast(t('clipboard_direction_save_failed'), 'error')
  } finally {
    savingDirection.value = false
  }
}

async function removeItem(item: IClipboardItem) {
  try { await deleteItem(item) } catch { toast(t('FAILED'), 'error') }
}
</script>
