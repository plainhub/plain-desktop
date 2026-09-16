<template>
  <v-modal @close="close">
    <template #headline>{{ $t('device_info') }}</template>
    <template #content>
      <ul v-if="peer" class="card list-items">
        <v-list-item :title="$t('ip_address')" :value="peer ? getPeerIp(peer) : ''" />
        <v-list-item :title="$t('port')" :value="String(peer.port)" />
        <v-list-item :title="$t('device_type')" :value="deviceTypeText" />
        <v-list-item v-if="statusText" :title="$t('status')">
          <template #value>
            <span class="chat-status-badge" :class="peer.status">{{ statusText }}</span>
          </template>
        </v-list-item>
      </ul>
    </template>
    <template #actions>
      <v-outlined-button @click="close">{{ $t('close') }}</v-outlined-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import type { IPeer } from '@/lib/interfaces'
import { popModal } from '@/components/modal'
import { DeviceType, PeerStatus } from '@/lib/status'
import { getPeerIp } from '@/lib/device/login-peers'

const { t } = useI18n()

const props = defineProps({
  peer: { type: Object as PropType<IPeer | null>, default: null },
})

const deviceTypeMap: Record<DeviceType, string> = {
  [DeviceType.NAS]: 'nas',
  [DeviceType.PHONE]: 'phone',
  [DeviceType.TABLET]: 'tablet',
  [DeviceType.COMPUTER]: 'computer',
  [DeviceType.TV]: 'tv',
  [DeviceType.OTHER]: 'other',
}

const statusMap: Record<PeerStatus, string> = {
  [PeerStatus.PAIRED]: 'paired',
  [PeerStatus.UNPAIRED]: 'unpaired',
  [PeerStatus.CHANNEL]: 'unpaired',
}

const deviceTypeText = computed(() => {
  const dt = props.peer?.deviceType
  if (!dt) return ''
  const key = deviceTypeMap[dt]
  return key ? t(key) : dt
})

const statusText = computed(() => {
  const st = props.peer?.status
  if (!st) return ''
  const key = statusMap[st]
  return key ? t(key) : ''
})

function close() {
  popModal()
}
</script>
